mod recent_view;

mod album_cell;
mod album_content_view;
mod artist_tag;
mod album_view;

mod artist_cell;
mod artist_content_view;
mod artist_view;

mod folder_view;

mod playlist_content_view;
mod playlist_view;
mod playlist_row;

mod dynamic_playlist_view;
mod dynamic_playlist_content_view;
mod dynamic_playlist_editor_view;
mod rule_button;
mod ordering_button;

// Common stuff shared between views
mod add_to_playlist;
mod generic_row;

// The Library controller itself
mod controller;

pub use recent_view::RecentView;

use album_cell::AlbumCell;
pub use album_content_view::AlbumContentView;
pub use album_view::AlbumView;

use artist_cell::ArtistCell;
pub use artist_content_view::ArtistContentView;
pub use artist_view::ArtistView;

pub use folder_view::FolderView;

pub use dynamic_playlist_view::DynamicPlaylistView;
pub use dynamic_playlist_content_view::DynamicPlaylistContentView;
pub use dynamic_playlist_editor_view::DynamicPlaylistEditorView;

pub use playlist_content_view::PlaylistContentView;
pub use playlist_view::PlaylistView;

pub use controller::Library;
