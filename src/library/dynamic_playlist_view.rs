use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{
    glib::{self, closure_local},
    CompositeTemplate, ListItem, SignalListItemFactory, SingleSelection,
};
use std::{cell::Cell, cmp::Ordering, rc::Rc};

use glib::clone;

use super::{Library, DynamicPlaylistContentView};
use crate::{
    cache::Cache,
    client::{ClientState, ConnectionState},
    common::INode,
    library::{playlist_row::PlaylistRow, DynamicPlaylistEditorView},
    utils::{g_cmp_str_options, g_search_substr, settings_manager},
    window::EuphonicaWindow
};

// DynamicPlaylist view implementation
mod imp {
    use std::{cell::OnceCell, sync::OnceLock};

    use glib::{subclass::Signal, Properties};

    // use crate::library::DynamicPlaylistContentView;

    use super::*;

    #[derive(Debug, CompositeTemplate, Properties, Default)]
    #[properties(wrapper_type = super::DynamicPlaylistView)]
    #[template(resource = "/io/github/htkhiem/Euphonica/gtk/library/dynamic-playlist-view.ui")]
    pub struct DynamicPlaylistView {
        #[template_child]
        pub nav_view: TemplateChild<adw::NavigationView>,
        #[template_child]
        pub show_sidebar: TemplateChild<gtk::Button>,

        // Search & filter widgets
        #[template_child]
        pub sort_dir: TemplateChild<gtk::Image>,
        #[template_child]
        pub sort_dir_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub sort_mode: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub search_btn: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub search_bar: TemplateChild<gtk::SearchBar>,
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,

        #[template_child]
        pub create_btn: TemplateChild<gtk::Button>,

        // Content
        #[template_child]
        pub list_view: TemplateChild<gtk::ListView>,
        #[template_child]
        pub content_page: TemplateChild<adw::NavigationPage>,
        #[template_child]
        pub content_view: TemplateChild<DynamicPlaylistContentView>,

        // Editor view is created on demand.

        // Search & filter models
        pub search_filter: gtk::CustomFilter,
        pub sorter: gtk::CustomSorter,
        // Keep last length to optimise search
        // If search term is now longer, only further filter still-matching
        // items.
        // If search term is now shorter, only check non-matching items to see
        // if they now match.
        pub last_search_len: Cell<usize>,
        pub library: OnceCell<Library>,
        pub cache: OnceCell<Rc<Cache>>,
        #[property(get, set)]
        pub collapsed: Cell<bool>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DynamicPlaylistView {
        const NAME: &'static str = "EuphonicaDynamicPlaylistView";
        type Type = super::DynamicPlaylistView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_layout_manager_type::<gtk::BinLayout>();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for DynamicPlaylistView {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            self.obj()
                .bind_property(
                    "collapsed",
                    &self.show_sidebar.get(),
                    "visible"
                )
                .sync_create()
                .build();

            self.show_sidebar.connect_clicked(clone!(
                #[weak(rename_to = this)]
                self,
                move |_| {
                    this.obj().emit_by_name::<()>("show-sidebar-clicked", &[]);
                }
            ));
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("show-sidebar-clicked").build(),
                ]
            })
        }
    }

    impl WidgetImpl for DynamicPlaylistView {}
}

glib::wrapper! {
    pub struct DynamicPlaylistView(ObjectSubclass<imp::DynamicPlaylistView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for DynamicPlaylistView {
    fn default() -> Self {
        Self::new()
    }
}

impl DynamicPlaylistView {
    pub fn new() -> Self {
        let res: Self = glib::Object::new();

        res
    }

    pub fn pop(&self) {
        self.imp().nav_view.pop();
    }

    pub fn setup(
        &self,
        library: Library,
        cache: Rc<Cache>,
        client_state: ClientState,
        window: &EuphonicaWindow,
    ) {
        let content_view = self.imp().content_view.get();
        content_view.setup(library.clone(), client_state.clone(), cache.clone());
        self.imp().content_page.connect_hidden(move |_| {
            content_view.unbind();
        });
        self.imp()
            .library
            .set(library.clone())
            .expect("Cannot init DynamicPlaylistView with Library");
        self.imp()
            .cache
            .set(cache.clone())
            .expect("Cannot init DynamicPlaylistView with cache controller");
        self.setup_sort();
        self.setup_search();
        self.setup_listview();

        client_state.connect_notify_local(
            Some("connection-state"),
            clone!(
                #[weak(rename_to = this)]
                self,
                move |state, _| {
                    if state.get_connection_state() == ConnectionState::Connected {
                        // Newly-connected? Get all playlists.
                        this.imp().library.get().unwrap().init_dyn_playlists(false);
                    }
                }
            ),
        );

        self.imp()
            .create_btn
            .connect_clicked(clone!(
                #[weak(rename_to = this)]
                self,
                #[weak]
                library,
                #[weak]
                cache,
                #[weak]
                client_state,
                #[weak]
                window,
                move |_| {
                    let editor = DynamicPlaylistEditorView::default();
                    editor.setup(library, cache, client_state, &window);
                    editor.connect_closure(
                        "exit-clicked",
                        false,
                        closure_local!(
                            #[weak]
                            this,
                            move |_: DynamicPlaylistEditorView, should_refresh: bool| {
                                this.imp().nav_view.pop_to_tag("list");
                                if should_refresh {
                                    this.imp().library.get().unwrap().init_dyn_playlists(true);
                                }
                            }
                        )
                    );

                    this.imp().nav_view.push(
                        &adw::NavigationPage::builder()
                            .tag("editor")
                            .title("New Dynamic Playlist")
                            .child(&editor)
                            .build()
                    );
                }
            ));
    }

    fn setup_sort(&self) {
        // Setup sort widget & actions
        let settings = settings_manager();
        let state = settings.child("state").child("dynplaylistview");
        let library_settings = settings.child("library");
        let sort_dir_btn = self.imp().sort_dir_btn.get();
        sort_dir_btn.connect_clicked(clone!(
            #[weak]
            state,
            move |_| {
                if state.string("sort-direction") == "asc" {
                    let _ = state.set_string("sort-direction", "desc");
                } else {
                    let _ = state.set_string("sort-direction", "asc");
                }
            }
        ));
        let sort_dir = self.imp().sort_dir.get();
        state
            .bind("sort-direction", &sort_dir, "icon-name")
            .get_only()
            .mapping(|dir, _| match dir.get::<String>().unwrap().as_ref() {
                "asc" => Some("view-sort-ascending-symbolic".to_value()),
                _ => Some("view-sort-descending-symbolic".to_value()),
            })
            .build();
        let sort_mode = self.imp().sort_mode.get();
        state
            .bind("sort-by", &sort_mode, "selected")
            .mapping(|val, _| {
                // TODO: i18n
                match val.get::<String>().unwrap().as_ref() {
                    "filename" => Some(0.to_value()),
                    "last-modified" => Some(1.to_value()),
                    _ => unreachable!(),
                }
            })
            .set_mapping(|val, _| match val.get::<u32>().unwrap() {
                0 => Some("filename".to_variant()),
                1 => Some("last-modified".to_variant()),
                _ => unreachable!(),
            })
            .build();
        self.imp().sorter.set_sort_func(clone!(
            #[strong]
            library_settings,
            #[strong]
            state,
            move |obj1, obj2| {
                let inode1 = obj1
                    .downcast_ref::<INode>()
                    .expect("Sort obj has to be a common::INode.");

                let inode2 = obj2
                    .downcast_ref::<INode>()
                    .expect("Sort obj has to be a common::INode.");

                // Should we sort ascending?
                let asc = state.enum_("sort-direction") > 0;
                // Should the sorting be case-sensitive, i.e. uppercase goes first?
                let case_sensitive = library_settings.boolean("sort-case-sensitive");
                // Should nulls be put first or last?
                let nulls_first = library_settings.boolean("sort-nulls-first");

                // Vary behaviour depending on sort menu
                match state.enum_("sort-by") {
                    // Refer to the io.github.htkhiem.Euphonica.sortby enum the gschema
                    6 => {
                        // Filename
                        g_cmp_str_options(
                            Some(inode1.get_uri()),
                            Some(inode2.get_uri()),
                            nulls_first,
                            asc,
                            case_sensitive,
                        )
                    }
                    7 => {
                        // Last modified
                        g_cmp_str_options(
                            inode1.get_last_modified(),
                            inode2.get_last_modified(),
                            nulls_first,
                            asc,
                            case_sensitive,
                        )
                    }
                    _ => unreachable!(),
                }
            }
        ));

        // Update when changing sort settings
        state.connect_changed(
            Some("sort-by"),
            clone!(
                #[weak(rename_to = this)]
                self,
                move |_, _| {
                    println!("Updating sort...");
                    this.imp().sorter.changed(gtk::SorterChange::Different);
                }
            ),
        );
        state.connect_changed(
            Some("sort-direction"),
            clone!(
                #[weak(rename_to = this)]
                self,
                move |_, _| {
                    println!("Flipping sort...");
                    // Don't actually sort, just flip the results :)
                    this.imp().sorter.changed(gtk::SorterChange::Inverted);
                }
            ),
        );
    }

    fn setup_search(&self) {
        let settings = settings_manager();
        let library_settings = settings.child("library");
        // Set up search filter
        self.imp().search_filter.set_filter_func(clone!(
            #[weak(rename_to = this)]
            self,
            #[strong]
            library_settings,
            #[upgrade_or]
            true,
            move |obj| {
                let inode = obj
                    .downcast_ref::<INode>()
                    .expect("Search obj has to be a common::INode.");

                let search_term = this.imp().search_entry.text();
                if search_term.is_empty() {
                    return true;
                }

                // Should the searching be case-sensitive?
                let case_sensitive = library_settings.boolean("search-case-sensitive");
                g_search_substr(Some(inode.get_uri()), &search_term, case_sensitive)
            }
        ));

        // Connect search entry to filter. Filter will later be put in GtkSearchModel.
        // That GtkSearchModel will listen to the filter's changed signal.
        let search_entry = self.imp().search_entry.get();
        search_entry.connect_search_changed(clone!(
            #[weak(rename_to = this)]
            self,
            move |entry| {
                let text = entry.text();
                let new_len = text.len();
                let old_len = this.imp().last_search_len.replace(new_len);
                match new_len.cmp(&old_len) {
                    Ordering::Greater => {
                        this.imp()
                            .search_filter
                            .changed(gtk::FilterChange::MoreStrict);
                    }
                    Ordering::Less => {
                        this.imp()
                            .search_filter
                            .changed(gtk::FilterChange::LessStrict);
                    }
                    Ordering::Equal => {
                        this.imp()
                            .search_filter
                            .changed(gtk::FilterChange::Different);
                    }
                }
            }
        ));
    }

    pub fn on_playlist_clicked(&self, inode: &INode) {
        let content_view = self.imp().content_view.get();
        content_view.unbind();
        content_view.bind_by_name(inode.get_uri());
        if self.imp().nav_view.visible_page_tag().is_none_or(|tag| tag.as_str() != "content") {
            self.imp().nav_view.push_by_tag("content");
        }
        // Unlike other views, DynamicPlaylistContentView initialises itself.
    }

    fn setup_listview(&self) {
        let library = self.imp().library.get().unwrap();
        let cache = self.imp().cache.get().unwrap();
        // Setup search bar
        let search_bar = self.imp().search_bar.get();
        let search_entry = self.imp().search_entry.get();
        search_bar.connect_entry(&search_entry);

        let search_btn = self.imp().search_btn.get();
        search_btn
            .bind_property("active", &search_bar, "search-mode-enabled")
            .sync_create()
            .build();

        // Chain search & sort. Put sort after search to reduce number of sort items.
        let playlists = library.dyn_playlists();
        let search_model = gtk::FilterListModel::new(
            Some(playlists.clone()),
            Some(self.imp().search_filter.clone()),
        );
        search_model.set_incremental(true);
        let sort_model =
            gtk::SortListModel::new(Some(search_model), Some(self.imp().sorter.clone()));
        sort_model.set_incremental(true);
        let sel_model = SingleSelection::new(Some(sort_model));

        self.imp().list_view.set_model(Some(&sel_model));

        // Set up factory
        let factory = SignalListItemFactory::new();

        factory.connect_setup(clone!(
            #[weak]
            library,
            #[weak]
            cache,
            move |_, list_item| {
                let item = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem");
                let folder_row = PlaylistRow::new(true, library, item, cache);
                item.set_child(Some(&folder_row));
            }
        ));

        factory.connect_bind(
            move |_, list_item| {
                let item = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem");

                let playlist = item
                    .item()
                    .and_downcast::<INode>()
                    .expect("The item has to be a common::INode.");

                let child: PlaylistRow = item
                    .child()
                    .and_downcast::<PlaylistRow>()
                    .expect("The child has to be an `DynamicPlaylistRow`.");

                child.bind(&playlist);
            }
        );

        // Set the factory of the list view
        self.imp().list_view.set_factory(Some(&factory));

        // Setup click action
        self.imp().list_view.connect_activate(clone!(
            #[weak(rename_to = this)]
            self,
            move |grid_view, position| {
                let model = grid_view.model().expect("The model has to exist.");
                let inode = model
                    .item(position)
                    .and_downcast::<INode>()
                    .expect("The item has to be a `common::INode`.");
                println!("Clicked on {:?}", &inode);
                this.on_playlist_clicked(&inode);
            }
        ));
    }
}
