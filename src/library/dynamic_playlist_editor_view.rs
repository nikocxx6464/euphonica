use adw::subclass::prelude::*;
use glib::{clone, closure_local, signal::SignalHandlerId, Binding};
use gtk::{gio, glib, gdk, prelude::*, BitsetIter, CompositeTemplate, ListItem, SignalListItemFactory};
use uuid::Uuid;
use std::{
    cell::{OnceCell, RefCell},
    rc::Rc,
};
use time::{format_description, Date};
use derivative::Derivative;
use strum::{EnumCount, IntoEnumIterator, VariantArray};

use super::{artist_tag::ArtistTag, ordering_button::OrderingButton, rule_button::RuleButton, AlbumSongRow, Library};
use crate::{
    cache::{
        placeholders::{ALBUMART_PLACEHOLDER, EMPTY_ALBUM_STRING},
        Cache, CacheState
    },
    client::ClientState,
    common::{
        dynamic_playlist::{AutoRefresh, Ordering, Rule},
        Album, AlbumInfo, Artist, CoverSource, DynamicPlaylist, Rating, Song
    },
    library::{DynamicPlaylistSongRow, PlaylistSongRow},
    utils::format_secs_as_duration,
    window::EuphonicaWindow
};

#[derive(Default, Debug, Clone)]
pub enum CoverPathAction {
    #[default]
    NoChange,
    New(String),
    Clear
}



mod imp {
    use std::cell::Cell;

    use ashpd::desktop::file_chooser::SelectedFiles;
    use async_channel::Sender;
    use gio::ListStore;
    use once_cell::sync::Lazy;

    use crate::{common::DynamicPlaylist, library::{ordering_button::OrderingButton, rule_button::RuleButton}, utils};

    use super::*;

    #[derive(Debug, CompositeTemplate, Derivative)]
    #[derivative(Default)]
    #[template(resource = "/io/github/htkhiem/Euphonica/gtk/library/dynamic-playlist-editor-view.ui")]
    pub struct DynamicPlaylistEditorView {
        #[template_child]
        pub edit_cover_dialog: TemplateChild<adw::Dialog>,
        #[template_child]
        pub set_cover_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub clear_cover_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub infobox_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub collapse_infobox: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub cover: TemplateChild<gtk::Image>,

        #[template_child]
        pub save_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub title: TemplateChild<gtk::Entry>,
        #[template_child]
        pub description: TemplateChild<gtk::Entry>,
        #[template_child]
        pub add_rule_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub rules_box: TemplateChild<adw::WrapBox>,
        pub rules_model: OnceCell<gio::ListModel>,
        #[template_child]
        pub add_ordering_btn: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub ordering_box: TemplateChild<adw::WrapBox>,
        pub orderings_model: OnceCell<gio::ListModel>,

        #[template_child]
        pub refresh_schedule: TemplateChild<gtk::DropDown>,

        #[template_child]
        pub limit_mode: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub limit: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub limit_unit: TemplateChild<gtk::Label>,

        #[template_child]
        pub track_count: TemplateChild<gtk::Label>,
        #[template_child]
        pub runtime: TemplateChild<gtk::Label>,
        #[template_child]
        pub refresh_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub content_pages: TemplateChild<gtk::Stack>,
        #[template_child]
        pub content: TemplateChild<gtk::ListView>,

        #[derivative(Default(value = "gio::ListStore::new::<Song>()"))]
        pub song_list: gio::ListStore,
        pub orderings_menu: gio::Menu,
        pub cover_action: RefCell<CoverPathAction>,
        pub rules_valid: Cell<bool>,
        pub title_valid: Cell<bool>,

        pub library: OnceCell<Library>,
        pub cache: OnceCell<Rc<Cache>>,
        #[derivative(Default(value = "RefCell::new(String::from(\"\"))"))]
        pub tmp_name: RefCell<String>,
        pub window: OnceCell<EuphonicaWindow>,
        pub filepath_sender: OnceCell<Sender<String>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DynamicPlaylistEditorView {
        const NAME: &'static str = "EuphonicaDynamicPlaylistEditorView";
        type Type = super::DynamicPlaylistEditorView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.set_layout_manager_type::<gtk::BinLayout>();
            // klass.set_css_name("albumview");
            klass.set_accessible_role(gtk::AccessibleRole::Group);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DynamicPlaylistEditorView {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.title.connect_changed(clone!(
                #[weak(rename_to = this)]
                self,
                move |_| {
                    this.obj().validate_title();
                }
            ));

            // TODO: find another way as observe_children() is very inefficient
            let _ = self.rules_model.set(
                self.rules_box.observe_children()
            );
            self.obj().validate_rules();

            self.add_rule_btn.connect_clicked(clone!(
                #[weak(rename_to = this)]
                self,
                move |add_btn| {
                    let rules_box = this.rules_box.get();
                    let btn = RuleButton::new(&rules_box);
                    rules_box.append(&btn);
                    rules_box.reorder_child_after(
                        add_btn,
                        Some(&btn)
                    );
                    btn.connect_notify_local(
                        Some("is-valid"),
                        clone!(
                            #[weak]
                            this,
                            move |_, _| {
                                this.obj().validate_rules();
                            }
                        )
                    );
                    // Validate once at creation
                    btn.validate();
                    this.obj().validate_rules();
                }
            ));

            let obj = self.obj();
            // TODO: find another way as observe_children() is very inefficient
            let orderings_model = self.ordering_box.observe_children();
            orderings_model.connect_items_changed(clone!(
                #[weak]
                obj,
                move |_, _, _, _| {
                    obj.on_ordering_changed();
                }
            ));
            let _ = self.orderings_model.set(orderings_model);

            let action_add_ordering = gio::ActionEntry::builder("add-ordering")
                .parameter_type(Some(&glib::VariantTy::UINT32))
                .activate(clone!(
                    #[weak]
                    obj,
                    move |_, _, idx: Option<&glib::Variant>| {
                        let ordering_box = obj.imp().ordering_box.get();
                        let idx = idx.unwrap().get::<u32>().unwrap();
                        let ordering = Ordering::VARIANTS[idx as usize];
                        let btn = OrderingButton::new(ordering);
                        btn.connect_clicked(clone!(
                            #[weak]
                            ordering_box,
                            move |btn| {
                                ordering_box.remove(btn);
                            }
                        ));
                        ordering_box.append(&btn);
                        let add_ordering_btn = obj.imp().add_ordering_btn.get();
                        ordering_box.reorder_child_after(
                            &add_ordering_btn,
                            Some(&btn)
                        );
                    })
                )
                .build();

            // Create a new action group and add actions to it
            let actions = gio::SimpleActionGroup::new();
            actions.add_action_entries([action_add_ordering]);
            obj.insert_action_group("dynamic-playlist-editor-view", Some(&actions));
            // Once the actions are in place we can construct the menu items
            // Call once to init ordering options menu
            obj.on_ordering_changed();
            self.add_ordering_btn.set_menu_model(Some(&self.orderings_menu));

            self.limit_mode
                .bind_property(
                    "selected",
                    &self.limit.get(),
                    "visible"
                )
                .transform_to(|_, idx: u32| { Some(idx > 0) })
                .sync_create()
                .build();

            self.limit_mode
                .bind_property(
                    "selected",
                    &self.limit_unit.get(),
                    "visible"
                )
                .transform_to(|_, idx: u32| { Some(idx > 0) })
                .sync_create()
                .build();

            self.refresh_btn.connect_clicked(clone!(
                #[weak(rename_to = this)]
                self,
                move |_| {
                    this.obj().preview_result();
                }
            ));

            let infobox_revealer = self.infobox_revealer.get();
            let collapse_infobox = self.collapse_infobox.get();
            collapse_infobox
                .bind_property("active", &infobox_revealer, "reveal-child")
                .transform_to(|_, active: bool| Some(!active))
                .transform_from(|_, active: bool| Some(!active))
                .bidirectional()
                .sync_create()
                .build();

            infobox_revealer
                .bind_property("child-revealed", &collapse_infobox, "icon-name")
                .transform_to(|_, revealed| {
                    if revealed {
                        return Some("up-symbolic");
                    }
                    Some("down-symbolic")
                })
                .sync_create()
                .build();

            self.obj().update_sensitivity();
        }
    }

    impl WidgetImpl for DynamicPlaylistEditorView {}
}

glib::wrapper! {
    pub struct DynamicPlaylistEditorView(ObjectSubclass<imp::DynamicPlaylistEditorView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gio::ActionGroup, gio::ActionMap;
}

impl Default for DynamicPlaylistEditorView {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl DynamicPlaylistEditorView {
    fn get_library(&self) -> Option<&Library> {
        self.imp().library.get()
    }

    fn get_cache(&self) -> Option<&Rc<Cache>> {
        self.imp().cache.get()
    }

    /// Set a user-selected path as the new local cover.
    // pub fn set_cover(&self, path: &str) {
    //     if let (Some(album), Some(library)) = (
    //         self.imp().album.borrow().as_ref(),
    //         self.imp().library.get()
    //     ) {
    //         library.set_cover(album.get_folder_uri(), path);
    //     }
    // }

    pub fn setup(&self, library: Library, cache: Rc<Cache>, client_state: ClientState, window: &EuphonicaWindow) {
        let cache_state = cache.get_cache_state();
        self.imp()
           .cache
           .set(cache)
           .expect("DynamicPlaylistEditorView cannot bind to cache");
        self.imp()
           .window
           .set(window.clone())
           .expect("DynamicPlaylistEditorView cannot bind to window");
        self.imp().library.set(library).expect("Could not register DynamicPlaylistEditorView with library controller");

        // cache_state.connect_closure(
        //     "album-art-downloaded",
        //     false,
        //     closure_local!(
        //         #[weak(rename_to = this)]
        //         self,
        //         move |_: CacheState, uri: String, thumb: bool, tex: gdk::Texture| {
        //             if thumb {
        //                 return;
        //             }
        //             if let Some(album) = this.imp().album.borrow().as_ref() {
        //                 if album.get_folder_uri() == &uri {
        //                     // Force update since we might have been using an embedded cover
        //                     // temporarily
        //                     this.update_cover(tex, CoverSource::Folder);
        //                 } else if this.imp().cover_source.get() != CoverSource::Folder {
        //                     if album.get_example_uri() == &uri {
        //                         this.update_cover(tex, CoverSource::Embedded);
        //                     }
        //                 }
        //             }
        //         }
        //     ),
        // );
        // cache_state.connect_closure(
        //     "album-art-cleared",
        //     false,
        //     closure_local!(
        //         #[weak(rename_to = this)]
        //         self,
        //         move |_: CacheState, uri: String| {
        //             if let Some(album) = this.imp().album.borrow().as_ref() {
        //                 match this.imp().cover_source.get() {
        //                     CoverSource::Folder => {
        //                         if album.get_folder_uri() == &uri {
        //                             this.clear_cover();
        //                         }
        //                     }
        //                     CoverSource::Embedded => {
        //                         if album.get_example_uri() == &uri {
        //                             this.clear_cover();
        //                         }
        //                     }
        //                     _ => {}
        //                 }
        //             }
        //         }
        //     ),
        // );

        client_state.connect_closure(
            "dynamic-playlist-songs-downloaded",
            false,
            closure_local!(
                #[weak(rename_to = this)]
                self,
                move |_: ClientState, name: String, songs: glib::BoxedAnyObject| {
                    if name == this.imp().tmp_name.borrow().as_str() {
                        this.add_songs(songs.borrow::<Vec<Song>>().as_ref());
                    }
                }
            ),
        );

        // Set up channel for listening to cover path dialog
        // It is in these situations that Rust's lack of a standard async library bites hard.
        let (sender, receiver) = async_channel::unbounded::<String>();
        let _ = self.imp().filepath_sender.set(sender);
        glib::MainContext::default().spawn_local(clone!(
            #[strong(rename_to = this)]
            self,
            async move {
                use futures::prelude::*;
                // Allow receiver to be mutated, but keep it at the same memory address.
                // See Receiver::next doc for why this is needed.
                let mut receiver = std::pin::pin!(receiver);

                while let Some(path) = receiver.next().await {
                    this.set_cover(&path);
                }
            }
        ));

        // Set up factory
        let factory = SignalListItemFactory::new();

        // Create an empty `AlbumSongRow` during setup
        factory.connect_setup(clone!(
            #[strong(rename_to = this)]
            self,
            move |_, list_item| {
                let item = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem");
                let row = DynamicPlaylistSongRow::new(
                    this.get_library().expect("Error: dynamic playlist editor was not bound to library controller").clone(),
                    &item,
                    this.get_cache().expect("Error: dynamic playlist editor was not bound to library controller").clone(),
                );
                item.set_child(Some(&row));
            }
        ));
        // Tell factory how to bind `AlbumSongRow` to one of our Album GObjects
        factory.connect_bind(move |_, list_item| {
            // Get `Song` from `ListItem` (that is, the data side)
            let item = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem");

            // Get `AlbumSongRow` from `ListItem` (the UI widget)
            let child: DynamicPlaylistSongRow = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<DynamicPlaylistSongRow>()
                .expect("The child has to be an `DynamicPlaylistSongRow`.");

            // Within this binding fn is where the cached album art texture gets used.
            child.bind(&item);
        });

        // When row goes out of sight, unbind from item to allow reuse with another.
        factory.connect_unbind(move |_, list_item| {
            // Get `DynamicPlaylistSongRow` from `ListItem` (the UI widget)
            let child: DynamicPlaylistSongRow = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<DynamicPlaylistSongRow>()
                .expect("The child has to be an `DynamicPlaylistSongRow`.");
            child.unbind();
        });

        // Set the factory of the list view
        self.imp().content.set_factory(Some(&factory));
        self.imp().content.set_model(Some(
            &gtk::NoSelection::new(Some(self.imp().song_list.clone()))
        ));
        // Setup click action
        // self.imp().content.connect_activate(clone!(
        //     #[strong(rename_to = this)]
        //     self,
        //     move |_, position| {
        //         if let (Some(album), Some(library)) = (
        //             this.imp().album.borrow().as_ref(),
        //             this.get_library()
        //         ) {
        //             library.queue_album(album.clone(), true, true, Some(position as u32));
        //         }
        //     }
        // ));
    }

    fn validate_title(&self) {
        let entry = self.imp().title.get();
        let is_valid = entry.text_length() > 0;
        let old_valid = self.imp().title_valid.replace(is_valid);


        if !is_valid && !entry.has_css_class("error") {
            entry.add_css_class("error");
        } else if is_valid && entry.has_css_class("error") {
            entry.remove_css_class("error");
        }

        if old_valid != is_valid {
            self.update_sensitivity();
        }
    }

    fn validate_rules(&self) {
        let model = self.imp().rules_model.get().unwrap();
        let n_items = model.n_items();
        // There's always the "add rule" button
        if n_items == 1 {
            // Allow fetching without any filters. Basically just sort the whole library.
            let old_rules_valid = self.imp().rules_valid.replace(true);
            if old_rules_valid {
                self.update_sensitivity();
            }
            return;
        }
        let mut per_rule_is_valid: Vec<bool> = Vec::with_capacity(n_items as usize);
        for i in 0..n_items {
            if let Some(rule_btn) = model.item(i).and_downcast_ref::<RuleButton>() {
                per_rule_is_valid.push(
                    rule_btn.is_valid()
                );
            }
        }
        let new_rules_valid = per_rule_is_valid
            .iter()
            .fold(true, |left, item| { left && *item });
        let old_rules_valid = self.imp().rules_valid.replace(new_rules_valid);
        if old_rules_valid != new_rules_valid {
            self.update_sensitivity();
        }
    }

    /// Never allow the ordering section to fall into an invalid state by enforcing
    /// the following rules:
    /// 1. If Random is added, disable the Add Ordering button as any further
    ///    ordering wouldn't make sense. We disable instead of hide to make this clearer.
    /// 2. If there's any non-Random option, disable the Random option in the Add
    ///    Ordering dropdown.
    /// 3. For every non-Random option, disable them in the dropdown too to prevent
    ///    adding duplicate ones.
    fn on_ordering_changed(&self) {
        let orderings = self.imp().orderings_model.get().unwrap();
        let mut presence = [false; Ordering::COUNT];
        let mut has_random = false;
        let mut has_other = false;
        let n_btns = orderings.n_items() as usize;
        for i in 0..n_btns {
            if let Some(ordering_btn) = orderings.item(i as u32).and_downcast::<OrderingButton>() {
                let ordering = ordering_btn.ordering();
                presence[ordering as usize] = true;
                // Some orderings cannot go together, so if one is presence, the other should
                // also be considered to be.
                if let Some(reversed) = ordering.reverse() {
                    presence[reversed as usize] = true;
                }
                if ordering == Ordering::Random {
                    has_random = true;
                } else {
                    has_other = true;
                }
            }
        }

        // If there's Random, disable Add Ordering button
        self.imp().add_ordering_btn.set_sensitive(!has_random);

        // Get remaining ordering options
        if !has_random {
            let orderings_menu = &self.imp().orderings_menu;
            orderings_menu.remove_all();
            for opt in Ordering::iter() {
                let idx = opt as usize;
                // Random option only makes sense when no other Ordering has been specified
                if !presence[idx] && (opt != Ordering::Random || !has_other) {
                    let item = gio::MenuItem::new(Some(opt.readable_name()), None);
                    item.set_action_and_target_value(
                        Some("dynamic-playlist-editor-view.add-ordering"),
                        Some(&(idx as u32).to_variant())
                    );
                    orderings_menu.append_item(&item);
                }
            }
        }
    }

    fn update_sensitivity(&self) {
        let rules_valid = self.imp().rules_valid.get();
        let title_valid = self.imp().title_valid.get();
        self.imp().save_btn.set_sensitive(rules_valid && title_valid);
        self.imp().refresh_btn.set_sensitive(rules_valid);
    }

    fn clear_cover(&self) {
        self.imp().cover_action.replace(CoverPathAction::Clear);
        self.imp().cover.set_paintable(Some(&*ALBUMART_PLACEHOLDER));
    }

    /// Set a user-selected path as the new local cover.
    pub fn set_cover(&self, path: &str) {
        self.imp().cover_action.replace(CoverPathAction::New(path.to_owned()));
        self.imp().cover.set_from_file(Some(path));
    }

    fn schedule_cover(&self, dp: &DynamicPlaylist) {
        self.imp().cover_action.replace(CoverPathAction::NoChange);
        self.imp().cover.set_paintable(Some(&*ALBUMART_PLACEHOLDER));
        if let Some(tex) = self
            .imp()
            .cache
            .get()
            .unwrap()
            .clone()
            .load_cached_playlist_cover(&dp.name, false) {
                self.imp().cover.set_paintable(Some(&tex));
            }
    }

    fn get_refresh_schedule(&self) -> AutoRefresh {
        // TODO: gettext
        match self
            .imp()
            .refresh_schedule
            .model()
            .and_downcast::<gtk::StringList>()
            .unwrap()
            .string(self.imp().refresh_schedule.selected())
            .unwrap()
            .as_str()
        {
            "No auto-refresh" => AutoRefresh::None,
            "Refresh hourly" => AutoRefresh::Hourly,
            "Refresh daily" => AutoRefresh::Daily,
            "Refresh weekly" => AutoRefresh::Weekly,
            "Refresh monthly" => AutoRefresh::Monthly,
            "Refresh yearly" => AutoRefresh::Yearly,
            _ => unimplemented!()
        }
    }

    fn preview_result(&self) {
        self.imp().refresh_btn.set_sensitive(false);
        self.imp().content_pages.set_visible_child_name("spinner");
        self.imp().song_list.remove_all();

        // Build a test DP instance with a random name to avoid confusion w/ late-coming
        // background fetches of actual DPs.
        let mut dp = self.build_dynamic_playlist();
        let tmp_name = Uuid::new_v4().simple().to_string();
        dp.name = tmp_name.clone();
        self.imp().tmp_name.replace(tmp_name);

        println!("{:?}", &dp);

        self.imp().library.get().unwrap().fetch_dynamic_playlist(dp, false, false);
    }

    fn build_dynamic_playlist(&self) -> DynamicPlaylist {
        let rules_model = self.imp().rules_model.get().unwrap();
        let n_rules = rules_model.n_items() as usize;
        let mut rules: Vec<Rule> = Vec::with_capacity(n_rules - 1);  // Except the "Add Rule" button
        for i in 0..n_rules {
            if let Some(rule_btn) = rules_model.item(i as u32).and_downcast_ref::<RuleButton>() {
                rules.push(
                    rule_btn.get_rule().unwrap()
                );
            }
        }

        let orderings_model = self.imp().orderings_model.get().unwrap();
        let n_orderings = orderings_model.n_items() as usize;
        let mut ordering: Vec<Ordering> = Vec::with_capacity(n_orderings - 1);  // Except the "Add Rule" button
        for i in 0..n_orderings {
            if let Some(ordering_btn) = orderings_model.item(i as u32).and_downcast::<OrderingButton>() {
                ordering.push(ordering_btn.ordering());
            }
        }
        let limit: Option<u32> = if self.imp().limit_mode.selected() > 0 {
            Some(self.imp().limit.adjustment().value().round() as u32)
        } else {
            None
        };

        DynamicPlaylist {
            name: self.imp().title.text().to_string(),
            description: self.imp().description.text().to_string(),
            last_queued: None,
            play_count: 0,
            rules,
            ordering,
            auto_refresh: self.get_refresh_schedule(),
            last_refresh: None,
            limit
        }
    }

    pub fn bind(&self, dp: DynamicPlaylist) {
        // let title_label = self.imp().title.get();
        // let artists_box = self.imp().artists_box.get();
        // let rating = self.imp().rating.get();
        // let release_date_label = self.imp().release_date.get();
        // let mut bindings = self.imp().bindings.borrow_mut();

        // let title_binding = album
        //     .bind_property("title", &title_label, "label")
        //     .transform_to(|_, s: Option<&str>| {
        //         Some(if s.is_none_or(|s| s.is_empty()) {
        //             (*EMPTY_ALBUM_STRING).to_value()
        //         } else {
        //             s.to_value()
        //         })
        //     })
        //     .sync_create()
        //     .build();
        // // Save binding
        // bindings.push(title_binding);

        // // Populate artist tags
        // let artist_tags = album.get_artists().iter().map(
        //     |info| ArtistTag::new(
        //         Artist::from(info.clone()),
        //         self.imp().cache.get().unwrap().clone(),
        //         self.imp().window.get().unwrap()
        //     )
        // ).collect::<Vec<ArtistTag>>();
        // self.imp().artist_tags.extend_from_slice(&artist_tags);
        // for tag in artist_tags {
        //     artists_box.append(&tag);
        // }

        // let rating_binding = album
        //     .bind_property("rating", &rating, "value")
        //     .sync_create()
        //     .build();
        // // Save binding
        // bindings.push(rating_binding);

        // self.update_meta(&album);
        // let release_date_binding = album
        //     .bind_property("release_date", &release_date_label, "label")
        //     .transform_to(|_, boxed_date: glib::BoxedAnyObject| {
        //         let format = format_description::parse("[year]-[month]-[day]")
        //             .ok()
        //             .unwrap();
        //         if let Some(release_date) = boxed_date.borrow::<Option<Date>>().as_ref() {
        //             return release_date.format(&format).ok();
        //         }
        //         Some("-".to_owned())
        //     })
        //     .sync_create()
        //     .build();
        // // Save binding
        // bindings.push(release_date_binding);

        // let release_date_viz_binding = album
        //     .bind_property("release_date", &release_date_label, "visible")
        //     .transform_to(|_, boxed_date: glib::BoxedAnyObject| {
        //         if boxed_date.borrow::<Option<Date>>().is_some() {
        //             return Some(true);
        //         }
        //         Some(false)
        //     })
        //     .sync_create()
        //     .build();
        // // Save binding
        // bindings.push(release_date_viz_binding);

        // let info = album.get_info();
        // self.schedule_cover(info);
        // self.imp().album.borrow_mut().replace(album);
    }

    pub fn unbind(&self) {
        // for binding in self.imp().bindings.borrow_mut().drain(..) {
        //     binding.unbind();
        // }

        // // Clear artists wrapbox. TODO: when adw 1.8 drops as stable please use remove_all() instead.
        // for tag in self.imp().artist_tags.iter::<gtk::Widget>() {
        //     self.imp().artists_box.remove(&tag.unwrap());
        // }
        // self.imp().artist_tags.remove_all();

        // if let Some(id) = self.imp().cover_signal_id.take() {
        //     if let Some(cache) = self.imp().cache.get() {
        //         cache.get_cache_state().disconnect(id);
        //     }
        // }
        // if let Some(_) = self.imp().album.take() {
        //     self.clear_cover();
        // }


        // Unset metadata widgets
        // self.imp().song_list.remove_all();
        // let content_spinner = self.imp().content_spinner.get();
        // if content_spinner.visible_child_name().unwrap() != "spinner" {
        //     content_spinner.set_visible_child_name("spinner");
        // }
        // let infobox_spinner = self.imp().infobox_spinner.get();
        // if infobox_spinner.visible_child_name().unwrap() != "spinner" {
        //     infobox_spinner.set_visible_child_name("spinner");
        // }
    }

    fn add_songs(&self, songs: &[Song]) {
        // If this is called with an empty list, it indicates the queried DP is empty.
        if songs.is_empty() {
            println!("dynamic_playlist_editor_view: end of response received");
            if self.imp().content_pages.visible_child_name().is_some_and(|name| name.as_str() != "empty")
                && self.imp().song_list.n_items() == 0
            {
                self.imp().content_pages.set_visible_child_name("empty");
            }
            self.imp().refresh_btn.set_sensitive(true);
        } else {
            self.imp().content_pages.set_visible_child_name("content");
            self.imp().song_list.extend_from_slice(songs);
        }
        // self.imp()
        //     .track_count
        //     .set_label(&self.imp().song_list.n_items().to_string());
        // self.imp().runtime.set_label(&format_secs_as_duration(
        //     self.imp()
        //         .song_list
        //         .iter()
        //         .map(|item: Result<Song, _>| {
        //             if let Ok(song) = item {
        //                 return song.get_duration();
        //             }
        //             0
        //         })
        //         .sum::<u64>() as f64,
        // ));
    }
}
