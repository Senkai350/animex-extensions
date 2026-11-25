//! Common AniMex DTOs: the Rust mirror of `wit/animex.wit` and
//! `src/types/index.ts`. Every IPC field is serialized in camelCase.

mod catalog;
mod details;
mod filters;
mod history;
mod library;
mod module;
mod player;
mod stream;
mod tracker;

pub use catalog::{AiringStatus, AnimeKind, AnimePreview, CatalogPage, CatalogParams, PageMeta};
pub use details::{
    AgeRating, AlternativeName, CalendarDate, Episode, EpisodeRef, Genre, InfoBlock, OriginalSource,
    RegionalPremiere, Release, ReleaseRef, TimeSkip, TimeSkipType,
    TrackerData,
};
pub use filters::{ChoicePresentation, FilterField, OptionPair};
pub use history::HistoryEntry;
pub use library::{Folder, FolderRole, FolderSummary, LibraryItem};
pub use module::{
    ModuleAppCompatibility, ModuleAuthor, ModuleDependency, ModuleKind, ModuleMeta,
    ModulePermissions,
};
pub use player::{Dub, PlaybackCandidate, PlayerCandidate, PlayerCapabilities};
pub use stream::{Header, PlaybackLink, Quality, StreamData, SubtitleTrack};
pub use tracker::{
    Character, MediaType, RelatedTitle, ScoreCount, Screenshot, SimilarTitle, StaffMember,
    StaffRef, StatisticsFeed, StatusCount, TitleCard, TitleFeed, TitleRelation, TitleStatistics,
    TrackerEpisode, TrackerEpisodePage, TrackerVideo, VideoKind,
};
