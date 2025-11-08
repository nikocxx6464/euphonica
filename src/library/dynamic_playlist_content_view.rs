use adw::subclass::prelude::*;
use glib::{clone, closure_local, signal::SignalHandlerId};
use gtk::{gio, glib, prelude::*, CompositeTemplate, ListItem, SignalListItemFactory};
use std::{
    cell::{OnceCell, RefCell},
    rc::Rc,
};
use derivative::Derivative;
use super::{artist_tag::ArtistTag, Library};
use crate::{
    cache::{placeholders::ALBUMART_PLACEHOLDER, sqlite, Cache},
    client::ClientState,
    common::{ContentView, Song, SongRow},
    utils::format_secs_as_duration,
};

mod imp {
    use crate::common::DynamicPlaylist;

    use super::*;

    #[derive(Debug, CompositeTemplate, Derivative)]
    #[derivative(Default)]
    #[template(resource = "/io/github/htkhiem/Euphonica/gtk/library/dynamic-playlist-content-view.ui")]
    pub struct DynamicPlaylistContentView {
        #[template_child]
        pub inner: TemplateChild<ContentView>,
        #[template_child]
        pub cover: TemplateChild<gtk::Image>,

        #[template_child]
        pub infobox_spinner: TemplateChild<gtk::Stack>,
        #[template_child]
        pub title: TemplateChild<gtk::Label>,

        #[template_child]
        pub last_modified: TemplateChild<gtk::Label>,
        #[template_child]
        pub track_count: TemplateChild<gtk::Label>,
        #[template_child]
        pub runtime: TemplateChild<gtk::Label>,

        #[template_child]
        pub replace_queue: TemplateChild<gtk::Button>,
        #[template_child]
        pub append_queue: TemplateChild<gtk::Button>,
        #[template_child]
        pub refresh_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub content_spinner: TemplateChild<gtk::Stack>,
        #[template_child]
        pub content: TemplateChild<gtk::ListView>,

        #[derivative(Default(value = "gio::ListStore::new::<Song>()"))]
        pub song_list: gio::ListStore,
        #[derivative(Default(value = "gio::ListStore::new::<ArtistTag>()"))]
        pub artist_tags: gio::ListStore,

        pub dp: RefCell<Option<DynamicPlaylist>>,
        pub library: OnceCell<Library>,
        pub cover_signal_id: RefCell<Option<SignalHandlerId>>,
        pub cache: OnceCell<Rc<Cache>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DynamicPlaylistContentView {
        const NAME: &'static str = "EuphonicaDynamicPlaylistContentView";
        type Type = super::DynamicPlaylistContentView;
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

    impl ObjectImpl for DynamicPlaylistContentView {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.content.set_model(Some(&gtk::NoSelection::new(Some(self.song_list.clone()))));

            self.refresh_btn.connect_clicked(clone!(
                #[weak(rename_to = this)]
                self,
                #[upgrade_or]
                (),
                move |_| {
                    if let (Some(library), Some(dp)) = (this.library.get(), this.dp.borrow_mut().as_ref()) {
                        let spinner = this.content_spinner.get();
                        if spinner.visible_child_name().unwrap() != "spinner" {
                            spinner.set_visible_child_name("spinner");
                        }
                        // Block queue actions while refreshing
                        this.append_queue.set_sensitive(false);
                        this.replace_queue.set_sensitive(false);
                        // Fetch from scratch & update cache
                        library.fetch_dynamic_playlist(dp.clone(), true);
                        // TODO: Update the "last refreshed" UI text
                    }
                }
            ));

            self.replace_queue.connect_clicked(clone!(
                #[weak(rename_to = this)]
                self,
                move |_| {
                    if let (Some(library), Some(dp)) = (this.library.get(), this.dp.borrow_mut().as_ref()) {
                        library.queue_cached_dynamic_playlist(&dp.name, true, true);
                    }
                }
            ));

            self.append_queue.connect_clicked(clone!(
                #[weak(rename_to = this)]
                self,
                move |_| {
                    if let (Some(library), Some(dp)) = (this.library.get(), this.dp.borrow_mut().as_ref()) {
                        library.queue_cached_dynamic_playlist(&dp.name, false, false);
                    }
                }
            ));
        }
    }

    impl WidgetImpl for DynamicPlaylistContentView {}

    impl DynamicPlaylistContentView {
    }
}

glib::wrapper! {
    pub struct DynamicPlaylistContentView(ObjectSubclass<imp::DynamicPlaylistContentView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for DynamicPlaylistContentView {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl DynamicPlaylistContentView {
    fn get_library(&self) -> Option<&Library> {
        self.imp().library.get()
    }

    pub fn setup(&self, library: Library, client_state: ClientState, cache: Rc<Cache>) {
        self.imp()
           .cache
           .set(cache)
           .expect("DynamicPlaylistContentView cannot bind to cache");
        self.imp().library.set(library.clone()).expect("Could not register album content view with library controller");

        client_state.connect_closure(
            "dynamic-playlist-songs-downloaded",
            false,
            closure_local!(
                #[weak(rename_to = this)]
                self,
                move |_: ClientState, name: &str, songs: glib::BoxedAnyObject| {
                    if let Some(dp) = this.imp().dp.borrow().as_ref() {
                        if dp.name == name {
                            this.add_songs(songs.borrow::<Vec<Song>>().as_ref());
                        }
                    }
                }
            ),
        );

        let replace_queue_btn = self.imp().replace_queue.get();
        client_state
            .bind_property("is-queuing", &replace_queue_btn, "sensitive")
            .invert_boolean()
            .sync_create()
            .build();
        let append_queue_btn = self.imp().append_queue.get();
        client_state
            .bind_property("is-queuing", &append_queue_btn, "sensitive")
            .invert_boolean()
            .sync_create()
            .build();

        // Set up factory
        let factory = SignalListItemFactory::new();

        // For now don't show album arts as most of the time songs in the same
        // album will have the same embedded art anyway.
        factory.connect_setup(
            move |_, list_item| {
                let item = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem");
                let row = SongRow::new(None);
                row.set_index_visible(true);
                row.set_thumbnail_visible(false);
                item.property_expression("item")
                    .chain_property::<Song>("track")
                    .bind(&row, "index", gtk::Widget::NONE);

                item.property_expression("item")
                    .chain_property::<Song>("name")
                    .bind(&row, "name", gtk::Widget::NONE);

                row.set_first_attrib_icon_name(Some("library-music-symbolic"));
                item.property_expression("item")
                    .chain_property::<Song>("album")
                    .bind(&row, "first-attrib-text", gtk::Widget::NONE);

                row.set_second_attrib_icon_name(Some("music-artist-symbolic"));
                item.property_expression("item")
                    .chain_property::<Song>("artist")
                    .bind(&row, "second-attrib-text", gtk::Widget::NONE);

                row.set_third_attrib_icon_name(Some("hourglass-symbolic"));
                item.property_expression("item")
                    .chain_property::<Song>("duration")
                    .chain_closure::<String>(closure_local!(|_: Option<glib::Object>, dur: u64| {
                        format_secs_as_duration(dur as f64)
                    }))
                    .bind(&row, "third-attrib-text", gtk::Widget::NONE);

                item.property_expression("item")
                    .chain_property::<Song>("quality-grade")
                    .bind(&row, "quality-grade", gtk::Widget::NONE);
                // No queue buttons here. We currently only support queuing the entire DP at once.
                item.set_child(Some(&row));
            }
        );
        // Tell factory how to bind `AlbumSongRow` to one of our Album GObjects
        factory.connect_bind(move |_, list_item| {
            // Get `Song` from `ListItem` (that is, the data side)
            let item: Song = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .item()
                .and_downcast::<Song>()
                .expect("The item has to be a common::Song.");

            // Get `SongRow` from `ListItem` (the UI widget)
            let child: SongRow = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<SongRow>()
                .expect("The child has to be an `SongRow`.");
            // Download album art
            child.on_bind(&item);
        });

        // When row goes out of sight, unbind from item to allow reuse with another.
        factory.connect_unbind(move |_, list_item| {
            // Get `AlbumSongRow` from `ListItem` (the UI widget)
            let child: SongRow = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<SongRow>()
                .expect("The child has to be an `SongRow`.");
            child.on_unbind();
        });

        // Set the factory of the list view
        self.imp().content.set_factory(Some(&factory));

        // Setup click action
        // self.imp().content.connect_activate(clone!(
        //     #[weak(rename_to = this)]
        //     self,
        //     #[upgrade_or]
        //     (),
        //     move |_, position| {
        //         if let (Some(album), Some(library)) = (
        //             this.imp().album.borrow().as_ref(),
        //             this.get_library()
        //         ) {
        //             library.queue_album(album.clone(), true, true, Some(position));
        //         }
        //     }
        // ));
    }

    fn clear_cover(&self) {
        self.imp().cover.set_paintable(Some(&*ALBUMART_PLACEHOLDER));
    }

    fn schedule_cover(&self, name: &str) {
        self.imp().cover.set_paintable(Some(&*ALBUMART_PLACEHOLDER));
        let handle = self.imp().cache.get().unwrap().load_cached_playlist_cover(
            name, true, false
        );

        glib::spawn_future_local(clone!(
            #[weak(rename_to = this)]
            self,
            async move {
                if let Some(tex) = handle.await.unwrap() {
                    this.imp().cover.set_paintable(Some(&tex));
                }
            }
        ));
    }

    pub fn bind_by_name(&self, name: &str) {
        self.schedule_cover(name);
        let name = name.to_string();
        glib::spawn_future_local(clone!(
            #[weak(rename_to = this)]
            self,
            #[upgrade_or]
            (),
            async move {
                match gio::spawn_blocking(move || {
                    sqlite::get_dynamic_playlist_info(&name)
                }).await.unwrap() {
                    Ok(Some(dp)) => {
                        let library = this.get_library().unwrap();
                        this.imp().title.set_label(&dp.name);
                        // If we've got a cached version, use it. Else
                        // resolve rules from scratch.
                        // TODO: Implement autorefresh.
                        if dp.last_refresh.is_some() {
                            library.fetch_cached_dynamic_playlist(&dp.name);
                        }
                        else {
                            library.fetch_dynamic_playlist(dp.clone(), true);
                        }
                        this.imp().dp.replace(Some(dp));
                    }
                    other => {
                        dbg!(other);
                    }
                }
            }
        ));
    }

    pub fn unbind(&self) {
        self.imp().song_list.remove_all();
        self.imp().title.set_label("");
        self.imp().dp.take();
        self.clear_cover();
        let content_spinner = self.imp().content_spinner.get();
        if content_spinner.visible_child_name().unwrap() != "spinner" {
            content_spinner.set_visible_child_name("spinner");
        }
    }

    fn add_songs(&self, songs: &[Song]) {
        let content_spinner = self.imp().content_spinner.get();
        if content_spinner.visible_child_name().unwrap() != "content" {
            content_spinner.set_visible_child_name("content");
        }
        self.imp().song_list.extend_from_slice(songs);
        self.imp()
            .track_count
            .set_label(&self.imp().song_list.n_items().to_string());
        self.imp().runtime.set_label(&format_secs_as_duration(
            self.imp()
                .song_list
                .iter()
                .map(|item: Result<Song, _>| {
                    if let Ok(song) = item {
                        return song.get_duration();
                    }
                    0
                })
                .sum::<u64>() as f64,
        ));
    }
}
