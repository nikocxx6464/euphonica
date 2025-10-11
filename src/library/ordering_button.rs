use gio::glib::WeakRef;
use glib::{clone, Object};
use gtk::{glib, prelude::*, subclass::prelude::*, CompositeTemplate};

use crate::common::dynamic_playlist::Ordering;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/io/github/htkhiem/Euphonica/gtk/library/ordering-button.ui")]
    pub struct OrderingButton {
        #[template_child]
        pub order_by: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub delete: TemplateChild<gtk::Button>,

        pub wrap_box: WeakRef<adw::WrapBox>,
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for OrderingButton {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphonicaOrderingButton";
        type Type = super::OrderingButton;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for OrderingButton {}

    // Trait shared by all widgets
    impl WidgetImpl for OrderingButton {}

    // Trait shared by all boxes
    impl BoxImpl for OrderingButton {}
}

glib::wrapper! {
    pub struct OrderingButton(ObjectSubclass<imp::OrderingButton>)
    @extends gtk::Box, gtk::Widget,
    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl OrderingButton {
    pub fn new(wrap_box: &adw::WrapBox) -> Self {
        let res: Self = Object::builder().build();
        res.imp().wrap_box.set(Some(wrap_box));

        res.imp().delete.connect_clicked(clone!(
            #[weak]
            res,
            move |_| {
                if let Some(wrap_box) = res.imp().wrap_box.upgrade() {
                    wrap_box.remove(&res);
                }
            }
        ));
        res
    }

    pub fn get_ordering(&self) -> Option<Ordering> {
        // TODO: gettext
        match self
            .imp()
            .order_by
            .model()
            .and_downcast::<gtk::StringList>()
            .unwrap()
            .string(self.imp().order_by.selected())
            .unwrap()
            .as_str()
        {
            "Random" => Some(Ordering::Random),
            "Last modified" => Some(Ordering::LastModified),
            "First modified" => Some(Ordering::FirstModified),
            "Desc. rating" => Some(Ordering::DescRating),
            "Asc. rating" => Some(Ordering::AscRating),
            _ => unimplemented!()
        }
    }
}
