use glib::{
    closure_local,
    Object,
    SignalHandlerId,
    ParamSpec,
    ParamSpecString,
    ParamSpecBoolean,
    ParamSpecObject
};
use gtk::{gdk, glib, prelude::*, subclass::prelude::*, CompositeTemplate};
use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::Rc,
};
use once_cell::sync::Lazy;

use crate::{
    cache::{placeholders::ALBUMART_THUMBNAIL_PLACEHOLDER, Cache, CacheState},
    common::{CoverSource, Song, SongInfo},
    utils::strip_filename_linux,
};

use super::QualityGrade;

// Wrapper around the common row object to implement song thumbnail fetch logic.
mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/io/github/htkhiem/Euphonica/gtk/song-row.ui")]
    pub struct SongRow {
        #[template_child]
        pub quality_grade: TemplateChild<gtk::Image>,
        #[template_child]
        pub center_box: TemplateChild<gtk::CenterBox>,
        #[template_child]
        pub thumbnail: TemplateChild<gtk::Image>,
        #[template_child]
        pub name: TemplateChild<gtk::Label>,
        #[template_child]
        pub first_attrib_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub first_attrib_text: TemplateChild<gtk::Label>,
        #[template_child]
        pub second_attrib_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub second_attrib_text: TemplateChild<gtk::Label>,
        #[template_child]
        pub third_attrib_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub third_attrib_text: TemplateChild<gtk::Label>,
        pub song: RefCell<Option<Song>>,
        pub thumbnail_signal_ids: RefCell<Option<(SignalHandlerId, SignalHandlerId)>>,
        pub cache: OnceCell<Rc<Cache>>,
        pub thumbnail_source: Cell<CoverSource>
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for SongRow {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphonicaSongRow";
        type Type = super::SongRow;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    // Trait shared by all GObjects
    impl ObjectImpl for SongRow {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecBoolean::builder("thumbnail-visible").build(),
                    ParamSpecString::builder("name").build(),
                    ParamSpecString::builder("quality-grade").build(),
                    ParamSpecString::builder("first-attrib-icon-name").build(),
                    ParamSpecString::builder("second-attrib-icon-name").build(),
                    ParamSpecString::builder("third-attrib-icon-name").build(),
                    ParamSpecString::builder("first-attrib-text").build(),
                    ParamSpecString::builder("second-attrib-text").build(),
                    ParamSpecString::builder("third-attrib-text").build(),
                    ParamSpecObject::builder::<gtk::Widget>("end-widget").build()
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            match pspec.name() {
                "thumbnail-visible" => self.thumbnail.is_visible().to_value(),
                "name" => self.name.label().to_value(),
                "quality-grade" => self.quality_grade.icon_name().to_value(),
                "first-attrib-icon-name" => self.first_attrib_icon.icon_name().to_value(),
                "second-attrib-icon-name" => self.second_attrib_icon.icon_name().to_value(),
                "third-attrib-icon-name" => self.third_attrib_icon.icon_name().to_value(),
                "first-attrib-text" => self.first_attrib_text.label().to_value(),
                "second-attrib-text" => self.second_attrib_text.label().to_value(),
                "third-attrib-text" => self.third_attrib_text.label().to_value(),
                "end-widget" => self.center_box.end_widget().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &ParamSpec) {
            let obj = self.obj();
            match pspec.name() {
                "thumbnail-visible" => {
                    if let Ok(vis) = value.get::<bool>() {
                        self.thumbnail.set_visible(vis);
                    }
                }
                "name" => {
                    if let Ok(name) = value.get::<&str>() {
                        self.name.set_label(name);
                    }
                }
                "quality-grade" => {
                    let maybe_icon = value.get::<&str>();
                    self.quality_grade.set_visible(maybe_icon.is_ok());
                    self.quality_grade.set_icon_name(maybe_icon.ok());
                }
                "first-attrib-icon-name" => {
                    obj.set_first_attrib_icon_name(value.get::<&str>().ok());
                }
                "second-attrib-icon-name" => {
                    obj.set_second_attrib_icon_name(value.get::<&str>().ok());
                }
                "third-attrib-icon-name" => {
                    obj.set_third_attrib_icon_name(value.get::<&str>().ok());
                }
                "first-attrib-text" => {
                    obj.set_first_attrib_text(value.get::<&str>().ok());
                }
                "second-attrib-text" => {
                    obj.set_second_attrib_text(value.get::<&str>().ok());
                }
                "third-attrib-text" => {
                    obj.set_third_attrib_text(value.get::<&str>().ok());
                }
                "end-widget" => {
                    obj.set_end_widget(value.get::<gtk::Widget>().ok().as_ref());
                }
                _ => unimplemented!(),
            }
        }

        fn dispose(&self) {
            if let Some((set_id, clear_id)) = self.thumbnail_signal_ids.take() {
                let cache_state = self.cache.get().unwrap().get_cache_state();
                cache_state.disconnect(set_id);
                cache_state.disconnect(clear_id);
            }
        }
    }

    // Trait shared by all widgets
    impl WidgetImpl for SongRow {}

    // Trait shared by all boxes
    impl BoxImpl for SongRow {}
}

// Common row widget for displaying a single song, used across the UI.
glib::wrapper! {
    pub struct SongRow(ObjectSubclass<imp::SongRow>)
    @extends gtk::Box, gtk::Widget,
    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl SongRow {
    pub fn new(
        cache: Rc<Cache>,
        // set_thumbnail_signal: &str,
        // clear_thumbnail_signal: &str
    ) -> Self {
        let res: Self = Object::builder().build();
        let cache_state = cache.get_cache_state();
        let _ = res.imp().cache.set(cache);
        let _ = res.imp().thumbnail_signal_ids.replace(Some((
            cache_state.connect_closure(
                // set_thumbnail_signal,
                "album-art-downloaded",
                false,
                closure_local!(
                    #[weak]
                    res,
                    move |_: CacheState, uri: &str, thumb: bool, tex: &gdk::Texture| {
                        if !thumb {
                            return;
                        }
                        // Match song URI first then folder URI. Only try to match by folder URI
                        // if we don't have a current thumbnail.
                        if let Some(song) = res.imp().song.borrow().as_ref() {
                            if uri == song.get_uri() {
                                // Force update since we might have been using a folder cover
                                // temporarily
                                res.update_thumbnail(tex, CoverSource::Embedded);
                            } else if res.imp().thumbnail_source.get() != CoverSource::Embedded
                                && strip_filename_linux(song.get_uri()) == uri {
                                    res.update_thumbnail(tex, CoverSource::Folder);
                                }
                        }
                    }
                ),
            ),
            cache_state.connect_closure(
                // clear_thumbnail_signal,
                "album-art-cleared",
                false,
                closure_local!(
                    #[weak]
                    res,
                    move |_: CacheState, uri: &str| {
                        if let Some(song) = res.imp().song.borrow().as_ref() {
                            match res.imp().thumbnail_source.get() {
                                CoverSource::Folder => {
                                    if strip_filename_linux(song.get_uri()) == uri {
                                        res.clear_thumbnail();
                                    }
                                }
                                CoverSource::Embedded => {
                                    if song.get_uri() == uri {
                                        res.clear_thumbnail();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                ),
            ),
        )));

        res
    }

    fn clear_thumbnail(&self) {
        self.imp().thumbnail_source.set(CoverSource::None);
        self.imp().thumbnail.set_paintable(Some(&*ALBUMART_THUMBNAIL_PLACEHOLDER));
    }

    fn schedule_thumbnail(&self, info: &SongInfo) {
        self.imp().thumbnail_source.set(CoverSource::Unknown);
        self.imp().thumbnail.set_paintable(Some(&*ALBUMART_THUMBNAIL_PLACEHOLDER));
        if let Some((tex, is_embedded)) = self
            .imp()
            .cache
            .get()
            .unwrap()
            .clone()
            .load_cached_embedded_cover(info, true, true) {
                self.imp().thumbnail.set_paintable(Some(&tex));
                self.imp().thumbnail_source.set(
                    if is_embedded {CoverSource::Embedded} else {CoverSource::Folder}
                );
            }
    }

    fn update_thumbnail(&self, tex: &gdk::Texture, src: CoverSource) {
        self.imp().thumbnail.set_paintable(Some(tex));
        self.imp().thumbnail_source.set(src);
    }

    pub fn on_bind(&self, song: &Song) {
        self.imp().song.replace(Some(song.clone()));
        self.schedule_thumbnail(song.get_info());
    }

    pub fn on_unbind(&self) {
        if let Some(_) = self.imp().song.take() {
            self.clear_thumbnail();
        }
    }

    pub fn set_name(&self, name: &str) {
        self.imp().name.set_label(name);
    }

    pub fn set_thumbnail_visible(&self, vis: bool) {
        self.imp().thumbnail.set_visible(vis);
    }

    pub fn set_quality_grade(&self, grade: QualityGrade) {
        let icon_name = grade.to_icon_name();
        self.imp().quality_grade.set_visible(icon_name.is_some());
        self.imp().quality_grade.set_icon_name(icon_name);
    }

    pub fn set_first_attrib_icon_name(&self, val: Option<&str>) {
        self.imp().first_attrib_icon.set_visible(val.is_some());
        self.imp().first_attrib_icon.set_icon_name(val);
    }

    pub fn set_second_attrib_icon_name(&self, val: Option<&str>) {
        self.imp().second_attrib_icon.set_visible(val.is_some());
        self.imp().second_attrib_icon.set_icon_name(val);
    }

    pub fn set_third_attrib_icon_name(&self, val: Option<&str>) {
        self.imp().third_attrib_icon.set_visible(val.is_some());
        self.imp().third_attrib_icon.set_icon_name(val);
    }

    pub fn set_first_attrib_text(&self, val: Option<&str>) {
        self.imp().first_attrib_text.set_visible(val.is_some());
        self.imp().first_attrib_text.set_label(val.unwrap_or(""));
    }

    pub fn set_second_attrib_text(&self, val: Option<&str>) {
        self.imp().second_attrib_text.set_visible(val.is_some());
        self.imp().second_attrib_text.set_label(val.unwrap_or(""));
    }

    pub fn set_third_attrib_text(&self, val: Option<&str>) {
        self.imp().third_attrib_text.set_visible(val.is_some());
        self.imp().third_attrib_text.set_label(val.unwrap_or(""));
    }

    pub fn set_end_widget(&self, widget: Option<&gtk::Widget>) {
        self.imp().center_box.set_end_widget(widget);
    }
}
