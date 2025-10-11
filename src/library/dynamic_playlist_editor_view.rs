use adw::subclass::prelude::*;
use gio::{ActionEntry, SimpleActionGroup, Menu};
use glib::{clone, closure_local, signal::SignalHandlerId, Binding};
use gtk::{gio, glib, gdk, prelude::*, BitsetIter, CompositeTemplate, ListItem, SignalListItemFactory};
use std::{
    cell::{OnceCell, RefCell},
    rc::Rc,
};
use time::{format_description, Date};
use derivative::Derivative;

use super::{artist_tag::ArtistTag, AlbumSongRow, Library};
use crate::{
    cache::{placeholders::{ALBUMART_PLACEHOLDER, EMPTY_ALBUM_STRING}, Cache, CacheState}, client::ClientState, common::{Album, AlbumInfo, Artist, CoverSource, DynamicPlaylist, Rating, Song}, library::PlaylistSongRow, utils::format_secs_as_duration, window::EuphonicaWindow
};

#[derive(Default, Debug, Clone)]
enum CoverPathAction {
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

    use crate::{common::DynamicPlaylist, library::{rule_button::RuleButton}, utils};

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
        pub cover_action: RefCell<CoverPathAction>,

        pub library: OnceCell<Library>,
        pub dp: RefCell<Option<DynamicPlaylist>>,
        pub window: OnceCell<EuphonicaWindow>,
        pub cache: OnceCell<Rc<Cache>>,
        pub filepath_sender: OnceCell<Sender<String>>
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
            self.add_rule_btn.connect_clicked(clone!(
                #[weak(rename_to = this)]
                self,
                move |_| {
                    let rules_box = this.rules_box.get();
                    rules_box.append(&RuleButton::new(&rules_box));
                }
            ));
        }
    }

    impl WidgetImpl for DynamicPlaylistEditorView {}
}

glib::wrapper! {
    pub struct DynamicPlaylistEditorView(ObjectSubclass<imp::DynamicPlaylistEditorView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
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

    /// Set a user-selected path as the new local cover.
    // pub fn set_cover(&self, path: &str) {
    //     if let (Some(album), Some(library)) = (
    //         self.imp().album.borrow().as_ref(),
    //         self.imp().library.get()
    //     ) {
    //         library.set_cover(album.get_folder_uri(), path);
    //     }
    // }

    pub fn setup(&self, library: Library, client_state: ClientState, cache: Rc<Cache>, window: &EuphonicaWindow) {
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
                    // TODO: disambiguate between this tentative playlist and existing ones by suffixing with "[EDITING]"
                    if let Some(dp) = this.imp().dp.borrow().as_ref() {
                        if dp.name == name {
                            this.add_songs(songs.borrow::<Vec<Song>>().as_ref());
                        }
                    }
                }
            ),
        );

        let infobox_revealer = self.imp().infobox_revealer.get();
        let collapse_infobox = self.imp().collapse_infobox.get();
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
                let row = PlaylistSongRow::new(
                    this.get_library().expect("Error: dynamic playlist editor was not bound to library controller").clone(),
                    this.as_ref(),
                    &item,
                    this.get_cache().expect("Error: dynamic playlist editor was not bound to library controller").clone(),
                );
                item.set_child(Some(&row));
            }
        ));
        // Tell factory how to bind `AlbumSongRow` to one of our Album GObjects
        factory.connect_bind(move |_, list_item| {
            // Get `Song` from `ListItem` (that is, the data side)
            let item: Song = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .item()
                .and_downcast::<Song>()
                .expect("The item has to be a common::Song.");

            // Get `AlbumSongRow` from `ListItem` (the UI widget)
            let child: AlbumSongRow = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<AlbumSongRow>()
                .expect("The child has to be an `AlbumSongRow`.");

            // Within this binding fn is where the cached album art texture gets used.
            child.bind(&item);
        });

        // When row goes out of sight, unbind from item to allow reuse with another.
        factory.connect_unbind(move |_, list_item| {
            // Get `AlbumSongRow` from `ListItem` (the UI widget)
            let child: AlbumSongRow = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<AlbumSongRow>()
                .expect("The child has to be an `AlbumSongRow`.");
            child.unbind();
        });

        // Set the factory of the list view
        self.imp().content.set_factory(Some(&factory));

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
        // let content_spinner = self.imp().content_spinner.get();
        // if content_spinner.visible_child_name().unwrap() != "content" {
        //     content_spinner.set_visible_child_name("content");
        // }
        // self.imp().song_list.extend_from_slice(songs);
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
