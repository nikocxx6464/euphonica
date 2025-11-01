pub mod song_row;
pub mod row_add_buttons;
pub mod row_edit_buttons;
pub mod album;
pub mod artist;
pub mod blend_mode;
pub mod inode;
pub mod marquee;
pub mod rating;
pub mod paintables;
pub mod song;
pub mod sticker;
pub mod theme_selector;
pub mod dynamic_playlist;

pub use song_row::SongRow;
pub use row_add_buttons::RowAddButtons;
pub use row_edit_buttons::RowEditButtons;
pub use sticker::Stickers;
pub use album::{Album, AlbumInfo};
pub use artist::{artists_to_string, parse_mb_artist_tag, Artist, ArtistInfo};
pub use inode::{INode, INodeType};
pub use marquee::Marquee;
pub use rating::Rating;
pub use song::{QualityGrade, Song, SongInfo};
pub use theme_selector::ThemeSelector;
pub use dynamic_playlist::DynamicPlaylist;


#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum CoverSource {
    Unknown,
    #[default]
    None,
    Folder,
    Embedded
}
