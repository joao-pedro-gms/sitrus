use std::time::Duration;

use sitrus_core::config::{Settings, Theme};
use sitrus_core::model::{Album, Artist, Artwork, EntityId, LibrarySection, Playlist, Song};
use sitrus_core::player_state::{PlaybackStatus, PlayerState};
use sitrus_core::queue::PlaybackQueue;
use sitrus_core::util::fmt_duration;

fn song(id: &str) -> Song {
    Song::new(EntityId::new(id), id, EntityId::new("album-1"))
}

#[test]
fn queue_empty_operations_are_safe() {
    let mut queue = PlaybackQueue::new();
    assert!(queue.is_empty());
    assert_eq!(queue.len(), 0);
    assert!(queue.current().is_none());
    assert!(queue.next().is_none());
    assert!(queue.previous().is_none());
    assert!(queue.remove(0).is_none());
    assert!(queue.reorder(0, 1).is_none());
}

#[test]
fn queue_with_one_item_starts_and_has_no_repeat() {
    let mut queue = PlaybackQueue::new();
    queue.add_last(song("only"));
    assert_eq!(queue.next().map(|item| item.id.as_str()), Some("only"));
    assert_eq!(queue.current().map(|item| item.id.as_str()), Some("only"));
    assert!(queue.next().is_none());
    assert_eq!(queue.current().map(|item| item.id.as_str()), Some("only"));
    assert!(queue.previous().is_none());
    assert_eq!(
        queue.remove(0).map(|item| item.id.as_str().to_owned()),
        Some("only".to_owned())
    );
    assert!(queue.is_empty());
    assert!(queue.current().is_none());
}

#[test]
fn queue_next_at_last_item_does_not_repeat() {
    let mut queue = PlaybackQueue::new();
    queue.add_last(song("first"));
    queue.add_last(song("last"));
    assert_eq!(queue.next().map(|item| item.id.as_str()), Some("first"));
    assert_eq!(queue.next().map(|item| item.id.as_str()), Some("last"));
    assert!(queue.next().is_none());
}

#[test]
fn queue_rejects_invalid_reorder_without_changing_items() {
    let mut queue = PlaybackQueue::new();
    queue.add_last(song("a"));
    queue.add_last(song("b"));
    assert!(queue.reorder(0, 2).is_none());
    assert_eq!(queue.remove(0).map(|item| item.id.0), Some("a".to_owned()));
}

#[test]
fn queue_supports_add_next_remove_and_reorder_sequence() {
    let mut queue = PlaybackQueue::new();
    queue.add_last(song("a"));
    queue.add_last(song("c"));
    assert_eq!(queue.next().map(|item| item.id.as_str()), Some("a"));
    queue.add_next(song("b"));
    assert_eq!(queue.next().map(|item| item.id.as_str()), Some("b"));
    assert_eq!(queue.reorder(2, 1), Some(()));
    assert_eq!(queue.current().map(|item| item.id.as_str()), Some("b"));
    assert_eq!(queue.previous().map(|item| item.id.as_str()), Some("c"));
    assert_eq!(queue.next().map(|item| item.id.as_str()), Some("b"));
    assert_eq!(
        queue.remove(0).map(|item| item.id.as_str().to_owned()),
        Some("a".to_owned())
    );
    assert_eq!(queue.len(), 2);
}

#[test]
fn player_state_accepts_valid_transitions_and_rejects_invalid_ones() {
    let mut player = PlayerState::new();
    assert!(player.transition_to(PlaybackStatus::Playing).is_err());
    assert_eq!(player.status(), PlaybackStatus::Stopped);
    player.transition_to(PlaybackStatus::Loading).unwrap();
    player.transition_to(PlaybackStatus::Playing).unwrap();
    player.transition_to(PlaybackStatus::Paused).unwrap();
    player.transition_to(PlaybackStatus::Playing).unwrap();
    player.transition_to(PlaybackStatus::Stopped).unwrap();
    assert_eq!(player.status(), PlaybackStatus::Stopped);
}

#[test]
fn player_state_can_recover_from_error_through_loading() {
    let mut player = PlayerState::new();
    player.transition_to(PlaybackStatus::Loading).unwrap();
    player.transition_to(PlaybackStatus::Error).unwrap();
    assert!(player.transition_to(PlaybackStatus::Playing).is_err());
    player.transition_to(PlaybackStatus::Stopped).unwrap();
    player.transition_to(PlaybackStatus::Loading).unwrap();
    player.transition_to(PlaybackStatus::Playing).unwrap();
}

#[test]
fn player_volume_is_clamped_to_unit_interval() {
    let mut player = PlayerState::new();
    player.set_volume(1.5);
    assert_eq!(player.volume(), 1.0);
    player.set_volume(-0.5);
    assert_eq!(player.volume(), 0.0);
    player.set_volume(0.4);
    assert_eq!(player.volume(), 0.4);
}

#[test]
fn player_tick_emits_end_event_when_duration_is_reached_or_passed() {
    let mut player = PlayerState::new();
    player.set_duration(Some(Duration::from_secs(10)));
    player.transition_to(PlaybackStatus::Loading).unwrap();
    player.transition_to(PlaybackStatus::Playing).unwrap();
    assert!(!player.tick(Duration::from_secs(7)));
    assert_eq!(player.position(), Duration::from_secs(7));
    assert!(player.tick(Duration::from_secs(5)));
    assert_eq!(player.position(), Duration::from_secs(10));
    assert_eq!(player.status(), PlaybackStatus::Stopped);
    assert!(!player.tick(Duration::from_secs(1)));
}

#[test]
fn fmt_duration_formats_short_and_long_values() {
    assert_eq!(fmt_duration(Duration::from_secs(187)), "3:07");
    assert_eq!(fmt_duration(Duration::from_secs(3_727)), "1:02:07");
    assert_eq!(fmt_duration(Duration::from_secs(9)), "0:09");
}

#[test]
fn config_defaults_and_automatic_list_threshold_are_available() {
    let settings: Settings = toml::from_str("").unwrap();
    assert_eq!(settings.theme, Theme::System);
    assert_eq!(settings.volume, 1.0);
    assert!(settings.automatic_list_mode);
    assert_eq!(settings.list_mode_threshold, 200);
    assert!(!settings.should_use_list_mode(200));
    assert!(settings.should_use_list_mode(201));
}

#[test]
fn domain_models_include_catalog_and_library_types() {
    let artist = Artist {
        id: EntityId::new("artist"),
        name: "Artist".to_owned(),
    };
    let album = Album {
        id: EntityId::new("album"),
        title: "Album".to_owned(),
        artist: artist.id.clone(),
    };
    let playlist = Playlist {
        id: EntityId::new("playlist"),
        title: "Favorites".to_owned(),
        songs: vec![EntityId::new("song")],
    };
    let artwork = Artwork {
        url: url::Url::parse("https://example.test/{w}x{h}bb.jpg").unwrap(),
    };
    assert_eq!(
        artwork.url_for_size(64).as_str(),
        "https://example.test/64x64bb.jpg"
    );
    assert_eq!(LibrarySection::Songs.as_str(), "songs");
    assert_eq!(album.artist, artist.id);
    assert_eq!(playlist.songs.len(), 1);
}
