//! Supplement events with categorical information.
//!
//! # Use cases for tags
//!
//! Using tags in trace data can improve readability by distinguishing
//! between different kinds of trace data such as requests, internal state,
//! or special operations. An error during a network request could mean a
//! timeout occurred, while an error in the internal state could mean
//! corruption. Both are errors, but one should be treated more seriously than
//! the other, and therefore the two should be easily distinguishable.
//!
//! # How to use tags
//!
//! Every application has its own preferences for how events should be tagged,
//! and this can be set via a custom [`TagParser`] in the [`ForestLayer`]. This
//! works by passing a reference to each incoming [`Event`] to the `TagParser`,
//! which can then be parsed into an `Option<Tag>` for the `ForestLayer` to use
//! later.
//!
//! Since [`TagParser`] is blanket implemented for all `Fn(&Event) -> Option<Tag>`
//! the easiest way to create one is to define a top-level function with this type
//! signature.
//!
//! Once the function is defined, it can either be passed directly to [`ForestLayer::new`],
//! or can be passed to [`LayerBuilder::set_tag`].
//!
//! [`ForestLayer`]: crate::layer::ForestLayer
//! [`ForestLayer::new`]: crate::layer::ForestLayer::new
//! [`LayerBuilder::set_tag`]: crate::builder::LayerBuilder::set_tag
//!
//! ## Examples
//!
//! Declaring and using a custom `TagParser`.
//! ```
//! use tracing_forest::{util::*, Tag};
//!
//! fn simple_tag(event: &Event) -> Option<Tag> {
//!     let target = event.metadata().target();
//!     let level = *event.metadata().level();
//!
//!     Some(match target {
//!         "security" if level == Level::ERROR => Tag::builder()
//!             .set_prefix(target)
//!             .set_suffix("critical")
//!             .set_icon('🔐')
//!             .finish(),
//!         "admin" | "request" => Tag::builder()
//!             .set_prefix(target)
//!             .set_level(level)
//!             .finish(),
//!         _ => return None,
//!     })
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     tracing_forest::worker_task()
//!         .set_tag(simple_tag)
//!         .build()
//!         .on(async {
//!             // Since `simple_tag` reads from the `target`, we use the target.
//!             // If it parsed the event differently, we would reflect that here.
//!             info!(target: "admin", "some info for the admin");
//!             error!(target: "request", "the request timed out");
//!             error!(target: "security", "the db has been breached");
//!             info!("no tags here");
//!         })
//!         .await;
//! }
//! ```
//! ```log
//! INFO     ｉ [admin.info]: some info for the admin
//! ERROR    🚨 [request.error]: the request timed out
//! ERROR    🔐 [security.critical]: the db has been breached
//! INFO     ｉ [info]: no tags here
//! ```
use crate::cfg_serde;
use std::fmt;
use tracing::{Event, Level};

/// A basic `Copy` type containing information about where an event occurred.
///
/// See the [module-level documentation](mod@crate::tag) for more details.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Tag {
    /// Optional prefix for the tag message
    prefix: Option<&'static str>,

    /// Level specifying the importance of the log.
    ///
    /// This value isn't necessarily "trace", "debug", "info", "warn", or "error",
    /// and can be customized.
    suffix: &'static str,

    /// An icon, typically emoji, that represents the tag.
    icon: char,
}

impl Tag {
    /// Create a new [`Builder`].
    pub const fn builder() -> Builder<Empty, Empty> {
        Builder {
            prefix: None,
            suffix: Empty(()),
            icon: Empty(()),
        }
    }

    /// Returns the prefix, if there is one.
    pub const fn prefix(&self) -> Option<&'static str> {
        self.prefix
    }

    /// Returns the suffix.
    pub const fn suffix(&self) -> &'static str {
        self.suffix
    }

    /// Returns the icon.
    pub const fn icon(&self) -> char {
        self.icon
    }
}

/// Incrementally construct [`Tag`]s.
///
/// This type uses generics to indicate when a field has been provided so that
/// [`Builder::finish`] can only be called once all the required fields have
/// been provided.
#[derive(Copy, Clone)]
pub struct Builder<S, I> {
    prefix: Option<&'static str>,
    suffix: S,
    icon: I,
}

/// A type used by [`Builder`] to indicate that a field hasn't been set.
#[derive(Copy, Clone)]
pub struct Empty(());

/// A type used by [`Builder`] to indicate that the suffix has been set.
#[derive(Copy, Clone)]
pub struct Suffix(&'static str);

/// A type used by [`Builder`] to indicate that the icon has been set.
#[derive(Copy, Clone)]
pub struct Icon(char);

impl<S, I> Builder<S, I> {
    /// Set the prefix.
    pub fn set_prefix(self, prefix: &'static str) -> Builder<S, I> {
        Builder {
            prefix: Some(prefix),
            ..self
        }
    }

    /// Set the suffix.
    pub fn set_suffix(self, suffix: &'static str) -> Builder<Suffix, I> {
        Builder {
            prefix: self.prefix,
            suffix: Suffix(suffix),
            icon: self.icon,
        }
    }

    /// Set the icon.
    pub fn set_icon(self, icon: char) -> Builder<S, Icon> {
        Builder {
            prefix: self.prefix,
            suffix: self.suffix,
            icon: Icon(icon),
        }
    }

    /// Set the suffix and icon using defaults for each [`Level`].
    ///
    /// If the `Tag` won't have a prefix, then `Tag::from(level)` can be used as
    /// a shorter alternative.
    pub fn set_level(self, level: Level) -> Builder<Suffix, Icon> {
        let (suffix, icon) = match level {
            Level::TRACE => ("trace", '📍'),
            Level::DEBUG => ("debug", '🐛'),
            Level::INFO => ("info", 'ｉ'),
            Level::WARN => ("warn", '🚧'),
            Level::ERROR => ("error", '🚨'),
        };

        Builder {
            prefix: self.prefix,
            suffix: Suffix(suffix),
            icon: Icon(icon),
        }
    }
}

impl Builder<Suffix, Icon> {
    /// Finish building a [`Tag`].
    ///
    /// Note that this can only be called once the suffix and icon have been provided.
    pub fn finish(self) -> Tag {
        Tag {
            prefix: self.prefix,
            suffix: self.suffix.0,
            icon: self.icon.0,
        }
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(prefix) = self.prefix {
            write!(f, "{}.{}", prefix, self.suffix)
        } else {
            self.suffix.fmt(f)
        }
    }
}

impl From<Level> for Tag {
    fn from(level: Level) -> Self {
        Tag::builder().set_level(level).finish()
    }
}

cfg_serde! {
    use serde::{Serialize, Serializer};

    impl Serialize for Tag {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            // This could probably go in a smart string
            serializer.serialize_str(&self.to_string())
        }
    }
}

/// A type that can parse [`Tag`]s from Tracing events.
///
/// This trait is blanket-implemented for all `Fn(&tracing::Event) -> Option<Tag>`,
/// so top-level `fn`s can be used.
///
/// See the [module-level documentation](mod@crate::tag) for more details.
pub trait TagParser: 'static {
    /// Parse a tag from a [`tracing::Event`]
    fn parse(&self, event: &Event) -> Option<Tag>;
}

/// A `TagParser` that always returns `None`.
#[derive(Clone, Debug)]
pub struct NoTag;

impl TagParser for NoTag {
    fn parse(&self, _event: &Event) -> Option<Tag> {
        None
    }
}

impl<F> TagParser for F
where
    F: 'static + Fn(&Event) -> Option<Tag>,
{
    fn parse(&self, event: &Event) -> Option<Tag> {
        self(event)
    }
}
