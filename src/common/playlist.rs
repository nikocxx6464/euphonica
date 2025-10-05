// use gtk::gdk::Texture;
// use gtk::glib;
// use gtk::prelude::*;
// use gtk::subclass::prelude::*;
// use std::cell::{OnceCell, RefCell};
// use time::Date;

// use crate::utils::strip_filename_linux;

// use super::{artists_to_string, parse_mb_artist_tag, ArtistInfo, QualityGrade, SongInfo, Stickers};

// #[derive(Debug, Clone, PartialEq)]
// pub struct PlaylistInfo {
//     pub title: String,
//     pub count: u32,
//     pub duration: u32,  // in seconds
//     pub last_modified: Option<String>
// }

// impl Default for PlaylistInfo {
//     fn default() -> Self {
//         PlaylistInfo {
//             title: "".to_owned(),
//             count: 0,
//             duration: 0,
//             last_modified: None
//         }
//     }
// }

// mod imp {
//     use super::*;
//     use glib::{
//         ParamSpec, ParamSpecChar, ParamSpecObject, ParamSpecString
//     };
//     use once_cell::sync::Lazy;

//     #[derive(Default, Debug)]
//     pub struct Playlist {
//         pub info: OnceCell<PlaylistInfo>
//     }

//     #[glib::object_subclass]
//     impl ObjectSubclass for Playlist {
//         const NAME: &'static str = "EuphonicaPlaylist";
//         type Type = super::Playlist;

//         fn new() -> Self {
//             Self {
//                 info: OnceCell::new()
//             }
//         }
//     }

//     impl ObjectImpl for Playlist {
//         fn properties() -> &'static [ParamSpec] {
//             static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
//                 vec![
//                     ParamSpecString::builder("title").read_only().build(),
//                     ParamSpecUInt::builder("count").read_only().build(),
//                     ParamSpecUInt::builder("duration").read_only().build(),
//                     ParamSpecString::builder("last-modified").read_only().build(),

//                 ]
//             });
//             PROPERTIES.as_ref()
//         }

//         fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
//             let info = self.info.get().unwrap();
//             match pspec.name() {
//                 "title" => info.title.to_value(),
//                 "count" => info.count.to_value(),
//                 "duration" => info.duration.to_value(),
//                 "last-modified" => info.last_modified.to_value(),
//                 _ => unimplemented!(),
//             }
//         }
//     }
// }

// glib::wrapper! {
//     pub struct Playlist(ObjectSubclass<imp::Playlist>);
// }

// impl Playlist {
//     // ALL of the getters below require that the info field be initialised!
//     pub fn get_info(&self) -> &PlaylistInfo {
//         &self.imp().info.get().unwrap()
//     }

//     pub fn get_title(&self) -> &str {
//         &self.get_info().title
//     }

//     pub fn get_sortable_title(&self) -> &str {
//         let info = self.get_info();
//         info.albumsort.as_deref().unwrap_or(info.title.as_str())
//     }

//     pub fn get_artists(&self) -> &[ArtistInfo] {
//         &self.get_info().artists
//     }

//     /// Get albumartist names separated by commas. If the first artist listed is a composer,
//     /// the next separator will be a semicolon instead. The quality of this output depends
//     /// on whether all delimiters are specified by the user.
//     pub fn get_artist_str(&self) -> Option<String> {
//         artists_to_string(&self.get_info().artists)
//     }

//     /// Get the original albumartist tag before any parsing.
//     pub fn get_artist_tag(&self) -> Option<&str> {
//         self.get_info().albumartist.as_deref()
//     }

//     /// Get the original ALBUMARTISTSORT tag.
//     pub fn get_sortable_artist_tag(&self) -> Option<&str> {
//         let info = self.get_info();
//         if let Some(albumartistsort) = info.albumartistsort.as_deref() {
//             Some(albumartistsort)
//         } else if let Some(albumartist) = info.albumartist.as_deref() {
//             Some(albumartist)
//         } else {
//             None
//         }
//     }

//     pub fn get_mbid(&self) -> Option<&str> {
//         self.get_info().mbid.as_deref()
//     }

//     pub fn get_release_date(&self) -> Option<Date> {
//         self.get_info().release_date.clone()
//     }

//     pub fn get_quality_grade(&self) -> QualityGrade {
//         self.get_info().quality_grade.clone()
//     }

//     pub fn get_rating(&self) -> Option<i8> {
//         self.imp().stickers.borrow().rating.clone()
//     }

//     pub fn set_rating(&self, new: Option<i8>) {
//         let old = self.get_rating();
//         if new != old {
//             self.imp().stickers.borrow_mut().rating = new;
//             self.notify("rating");
//         }
//     }

//     pub fn get_stickers(&self) -> &RefCell<Stickers> {
//         &self.imp().stickers
//     }
// }

// impl Default for Playlist {
//     fn default() -> Self {
//         glib::Object::new()
//     }
// }

// impl From<PlaylistInfo> for Playlist {
//     fn from(info: PlaylistInfo) -> Self {
//         let res = glib::Object::builder::<Self>().build();
//         let _ = res.imp().info.set(info);
//         res
//     }
// }
