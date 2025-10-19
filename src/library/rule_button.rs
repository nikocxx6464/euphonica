use gio::glib::WeakRef;
use glib::{prelude::*, clone, Object};
use gtk::{glib, prelude::*, subclass::prelude::*, CompositeTemplate};
use mpd::search::Operation as TagOperation;
use once_cell::sync::Lazy;
use std::cell::{OnceCell, RefCell};

use crate::common::{dynamic_playlist::{QueryLhs, Rule, StickerObjectType, StickerOperation}, INode, Stickers};

use super::Library;

mod imp {
    use std::{cell::Cell, ops::{Range, RangeBounds}, str::FromStr, sync::OnceLock};

    use gio::glib::{subclass::Signal, ParamSpecBoolean};
    use ::glib::Properties;
    use glib::{ParamSpec, ParamSpecEnum, ParamSpecString};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Default, CompositeTemplate, Properties)]
    #[properties(wrapper_type = super::RuleButton)]
    #[template(resource = "/io/github/htkhiem/Euphonica/gtk/library/rule-button.ui")]
    pub struct RuleButton {
        #[template_child]
        pub rule_type: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub op: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub lhs: TemplateChild<gtk::Entry>,
        #[template_child]
        pub rhs: TemplateChild<gtk::Entry>,
        #[template_child]
        pub delete: TemplateChild<gtk::Button>,

        pub wrap_box: WeakRef<adw::WrapBox>,
        #[property(get)]
        pub is_valid: Cell<bool>
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for RuleButton {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphonicaRuleButton";
        type Type = super::RuleButton;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for RuleButton {
        fn constructed(&self) {
            self.parent_constructed();

            self.rule_type.set_model(Some(&gtk::StringList::new(
                Self::rule_type_model()
            )));

            // Operator segment is applicable for
            // - Album rating (all numeric operators),
            // - URI (starts with; ==),
            // - Last modified (>=, <=, last n days),
            // - Tags (all tag operators)

            self.on_rule_type_changed();
            self.rule_type.connect_selected_notify(clone!(
                #[weak(rename_to = this)]
                self,
                move |_| {
                    this.on_rule_type_changed();
                }
            ));
            self.lhs.connect_changed(clone!(
                #[weak(rename_to = this)]
                self,
                move |_| {
                    this.validate();
                }
            ));
            self.rhs.connect_changed(clone!(
                #[weak(rename_to = this)]
                self,
                move |_| {
                    this.validate();
                }
            ));
        }

        // fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
        //     match pspec.name() {
        //         "uri" => self.uri.borrow().to_value(),
        //         "last-modified" => self.last_modified.label().to_value(),
        //         "inode-type" => self.inode_type.get().to_value(),
        //         _ => unimplemented!(),
        //     }
        // }

        // fn set_property(&self, _id: usize, value: &glib::Value, pspec: &ParamSpec) {
        //     match pspec.name() {
        //         "uri" => {
        //             if let Ok(name) = value.get::<&str>() {
        //                 // Keep display name synchronised
        //                 if let Some(title) = name.split('/').last() {
        //                     self.title.set_label(title);
        //                 }
        //                 self.uri.replace(name.to_string());
        //             } else {
        //                 self.title.set_label("");
        //             }
        //         }
        //         "last-modified" => {
        //             if let Ok(lm) = value.get::<&str>() {
        //                 self.last_modified.set_label(lm);
        //             } else {
        //                 self.last_modified.set_label("");
        //             }
        //         }
        //         "inode-type" => {
        //             if let Ok(it) = value.get::<INodeType>() {
        //                 self.inode_type.replace(it);
        //                 self.thumbnail.set_icon_name(Some(it.icon_name()));
        //                 if it == INodeType::Folder
        //                     || it == INodeType::Song
        //                     || it == INodeType::Playlist
        //                 {
        //                     self.replace_queue.set_visible(true);
        //                     self.append_queue.set_visible(true);
        //                 } else {
        //                     self.replace_queue.set_visible(false);
        //                     self.append_queue.set_visible(false);
        //                 }
        //                 // TODO: playlists support
        //             } else {
        //                 self.thumbnail
        //                     .set_icon_name(Some(&INodeType::default().icon_name()));
        //                 self.replace_queue.set_visible(false);
        //                 self.append_queue.set_visible(false);
        //             }
        //         }
        //         _ => unimplemented!(),
        //     }
        // }
    }

    // Trait shared by all widgets
    impl WidgetImpl for RuleButton {}

    // Trait shared by all boxes
    impl BoxImpl for RuleButton {}

    impl RuleButton {
        // TODO: gettext
        pub fn rule_type_model() -> &'static [&'static str] {
            static MODEL: Lazy<Vec<&str>> = Lazy::new(|| {
                vec![
                    "Rating",
                    "Album rating",
                    "URI",
                    "Modified within last",
                    "Played within last",
                    "Play count",
                    "Skipped within last",
                    "Skip count",
                    "Any tag",
                    "Tag: Album",
                    "Tag: Artist",
                    "Tag: AlbumArtist"
                ]
            });

            MODEL.as_ref()
        }

        pub fn tag_operator_model() -> &'static [&'static str] {
            static MODEL: Lazy<Vec<&str>> = Lazy::new(|| {
                vec![
                    "==",
                    "!=",
                    "contains",
                    "starts with"
                ]
            });

            MODEL.as_ref()
        }

        pub fn numeric_sticker_operator_model() -> &'static [&'static str] {
            static MODEL: Lazy<Vec<&str>> = Lazy::new(|| {
                vec![
                    "==",
                    ">",
                    "<"
                ]
            });

            MODEL.as_ref()
        }

        pub fn text_sticker_operator_model() -> &'static [&'static str] {
            static MODEL: Lazy<Vec<&str>> = Lazy::new(|| {
                vec![
                    "==",
                    ">",
                    "<",
                    "contains",
                    "starts with"
                ]
            });

            MODEL.as_ref()
        }

        pub fn uri_operator_model() -> &'static [&'static str] {
            static MODEL: Lazy<Vec<&str>> = Lazy::new(|| {
                vec![
                    "==",
                    "starts with"
                ]
            });

            MODEL.as_ref()
        }

        pub fn recency_operator_model() -> &'static [&'static str] {
            static MODEL: Lazy<Vec<&str>> = Lazy::new(|| {
                vec![
                    "days",
                    "weeks"
                ]
            });

            MODEL.as_ref()
        }

        pub fn on_rule_type_changed(&self) {
            let op_model: Option<gtk::StringList>;
            let lhs = self.lhs.get();
            let rhs = self.rhs.get();
            // Matching by string is more manageable in terms of future extensibility
            match self.obj().get_rule_type() {
                "Rating" | "Album rating" | "Play count" | "Skip count" => {
                    op_model = Some(
                        gtk::StringList::new(
                            Self::numeric_sticker_operator_model()
                        )
                    );
                    lhs.set_visible(false);
                    rhs.set_visible(true);
                    rhs.set_max_width_chars(3);
                    rhs.set_max_length(3);
                }
                "URI" => {
                    op_model = Some(
                        gtk::StringList::new(
                            Self::uri_operator_model()
                        )
                    );
                    lhs.set_visible(false);
                    rhs.set_visible(true);
                    rhs.set_max_width_chars(16);
                    rhs.set_max_length(0);
                },
                "Modified within last" | "Played within last" | "Skipped within last" => {
                    op_model = Some(
                        gtk::StringList::new(
                            Self::recency_operator_model()
                        )
                    );
                    lhs.set_visible(true);
                    lhs.set_max_width_chars(4);
                    lhs.set_max_length(4);
                    rhs.set_visible(false);
                },
                "Any tag" | "Tag: Album" | "Tag: Artist"
                    | "Tag: AlbumArtist" => {
                        op_model = Some(
                            gtk::StringList::new(
                                Self::tag_operator_model()
                            )
                        );
                        lhs.set_visible(false);
                        rhs.set_visible(true);
                        rhs.set_max_width_chars(16);
                        rhs.set_max_length(0);
                    },
                _ => {
                    op_model = None;
                }
            };
            self.validate();
            self.op.set_model(op_model.as_ref());
            self.op.set_visible(op_model.is_some());
        }

        pub fn validate(&self) {
            let is_valid = match self.obj().get_rule_type() {
                "Rating" | "Album rating" => self.rhs_is_numeric((0.0 as f64)..=(5.0 as f64)),
                "Play count" | "Skip count" => self.rhs_is_numeric((0.0 as u64)..),
                "URI" => self.rhs_is_nonempty(),
                "Modified within last" | "Played within last" | "Skipped within last" => self.lhs_is_numeric((0 as i64)..(3153600000 as i64)),  // Victorians didn't run Unix
                "Any tag" | "Tag: Album" | "Tag: Artist"
                    | "Tag: AlbumArtist" => self.rhs_is_nonempty(),
                _ => unimplemented!()
            };
            let old_valid = self.is_valid.replace(is_valid);
            if old_valid != is_valid {
                self.obj().notify("is-valid");
            }
        }

        fn lhs_is_numeric<T: Sized + FromStr + PartialOrd, R: RangeBounds<T>>(&self, range: R) -> bool {
            let entry = self.lhs.get();
            let text = entry.text();
            let is_valid = !text.is_empty() && text.parse::<T>().is_ok_and(|num| range.contains(&num));
            if !is_valid && !entry.has_css_class("error") {
                entry.add_css_class("error");
            } else if is_valid && entry.has_css_class("error") {
                entry.remove_css_class("error");
            }
            is_valid
        }

        fn rhs_is_numeric<T: Sized + FromStr + PartialOrd, R: RangeBounds<T>>(&self, range: R) -> bool {
            let entry = self.rhs.get();
            let text = entry.text();
            let is_valid = !text.is_empty() && text.parse::<T>().is_ok_and(|num| range.contains(&num));
            if !is_valid && !entry.has_css_class("error") {
                entry.add_css_class("error");
            } else if is_valid && entry.has_css_class("error") {
                entry.remove_css_class("error");
            }
            is_valid
        }

        fn rhs_is_nonempty(&self) -> bool {
            let entry = self.rhs.get();
            let is_err = entry.text().is_empty();
            if is_err && !entry.has_css_class("error") {
                entry.add_css_class("error");
            } else if !is_err && entry.has_css_class("error") {
                entry.remove_css_class("error");
            }
            !is_err
        }
    }
}

glib::wrapper! {
    pub struct RuleButton(ObjectSubclass<imp::RuleButton>)
    @extends gtk::Box, gtk::Widget,
    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl RuleButton {
    pub fn new(wrap_box: &adw::WrapBox) -> Self {
        let res: Self = Object::builder().build();
        res.imp().wrap_box.set(Some(wrap_box));

        res.imp().delete.connect_clicked(clone!(
            #[weak]
            res,
            move |_| {
                if let Some(wrap_box) = res.imp().wrap_box.upgrade() {
                    // Notify once to decrement error count in editor if this is an invalid rule
                    let old_valid = res.imp().is_valid.replace(true);
                    if !old_valid {
                        res.notify("is-valid");
                    }
                    wrap_box.remove(&res);
                }
            }
        ));
        res
    }

    pub fn get_rule_type(&self) -> &'static str {
        imp::RuleButton::rule_type_model()[self.imp().rule_type.selected() as usize]
    }

    pub fn get_rule(&self) -> Option<Rule> {
        if !self.is_valid() {
            None
        } else {
            match self.get_rule_type() {
                "Rating" => {
                    let internal_val = format!(
                        "{:.0}",
                        (self.imp().rhs.text().parse::<f64>().unwrap() * 2.0).round() as u8
                    );
                    Some(self.get_numeric_sticker_rule(StickerObjectType::Song, Stickers::RATING_KEY, internal_val))
                }
                "Album rating" => {
                    let internal_val = format!(
                        "{:.0}",
                        (self.imp().rhs.text().parse::<f64>().unwrap() * 2.0).round() as u8
                    );
                    Some(self.get_numeric_sticker_rule(StickerObjectType::Album, Stickers::RATING_KEY, internal_val))
                }
                "URI" => {
                    let lhs: QueryLhs = match imp
                        ::RuleButton
                        ::uri_operator_model()[self.imp().op.selected() as usize]
                    {
                        "==" => QueryLhs::File,
                        "starts with" => QueryLhs::Base,
                        _ => unimplemented!()
                    };
                    Some(Rule::Query(lhs, self.imp().rhs.text().to_string()))
                }
                "Modified within last" => {
                    let mul: i64 = match imp
                        ::RuleButton
                        ::recency_operator_model()[self.imp().op.selected() as usize]
                    {
                        "days" => 86400,
                        "weeks" => 604800,
                        _ => unimplemented!()
                    };
                    let secs = mul * self.imp().lhs.text().parse::<i64>().unwrap();
                    Some(Rule::LastModified(secs))
                }
                "Played within last" => {
                    let mul: i64 = match imp
                        ::RuleButton
                        ::recency_operator_model()[self.imp().op.selected() as usize]
                    {
                        "days" => 86400,
                        "weeks" => 604800,
                        _ => unimplemented!()
                    };
                    let secs = mul * self.imp().lhs.text().parse::<i64>().unwrap();
                    Some(Rule::Sticker(
                        StickerObjectType::Song,
                        Stickers::LAST_PLAYED_KEY.to_string(),
                        StickerOperation::IntGreaterThan,
                        secs.to_string()
                    ))
                }
                "Skipped within last" => {
                    let mul: i64 = match imp
                        ::RuleButton
                        ::recency_operator_model()[self.imp().op.selected() as usize]
                    {
                        "days" => 86400,
                        "weeks" => 604800,
                        _ => unimplemented!()
                    };
                    let secs = mul * self.imp().lhs.text().parse::<i64>().unwrap();
                    Some(Rule::Sticker(
                        StickerObjectType::Song,
                        Stickers::LAST_SKIPPED_KEY.to_string(),
                        StickerOperation::IntGreaterThan,
                        secs.to_string()
                    ))
                }
                "Play count" => {
                    let internal_val = format!(
                        "{:.0}",
                        (self.imp().rhs.text().parse::<f64>().unwrap()).round() as u64
                    );
                    Some(self.get_numeric_sticker_rule(StickerObjectType::Song, Stickers::PLAY_COUNT_KEY, internal_val))
                }
                "Skip count" => {
                    let internal_val = format!(
                        "{:.0}",
                        (self.imp().rhs.text().parse::<f64>().unwrap()).round() as u64
                    );
                    Some(self.get_numeric_sticker_rule(StickerObjectType::Song, Stickers::SKIP_COUNT_KEY, internal_val))
                }
                "Any tag" => {
                    let op = self.get_tag_op();
                    Some(Rule::Query(QueryLhs::Any(op), self.imp().rhs.text().to_string()))
                }
                "Tag: Album" => {
                    let op = self.get_tag_op();
                    Some(Rule::Query(QueryLhs::Album(op), self.imp().rhs.text().to_string()))
                }
                "Tag: Artist" => {
                    let op = self.get_tag_op();
                    Some(Rule::Query(QueryLhs::Artist(op), self.imp().rhs.text().to_string()))
                }
                "Tag: AlbumArtist" => {
                    let op = self.get_tag_op();
                    Some(Rule::Query(QueryLhs::AlbumArtist(op), self.imp().rhs.text().to_string()))
                }
                _ => unimplemented!()
            }
        }
    }

    pub fn validate(&self) {
        self.imp().validate();
    }

    fn get_tag_op(&self) -> TagOperation {
        match imp
            ::RuleButton
            ::tag_operator_model()[self.imp().op.selected() as usize]
        {
            "==" => TagOperation::Equals,
            "!=" => TagOperation::NotEquals,
            "contains" => TagOperation::Contains,
            "starts with" => TagOperation::StartsWith,
            _ => unimplemented!()
        }
    }

    fn get_numeric_sticker_rule(&self, obj_type: StickerObjectType, key: &str, val: String) -> Rule {

        let op: StickerOperation = match imp
            ::RuleButton
            ::numeric_sticker_operator_model()[self.imp().op.selected() as usize]
        {
            "==" => StickerOperation::IntEquals,
            ">" => StickerOperation::IntGreaterThan,
            "<" => StickerOperation::IntLessThan,
            _ => unimplemented!()
        };
        Rule::Sticker(obj_type, String::from(key), op, val)
    }
}
