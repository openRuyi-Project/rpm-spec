//! Top-level sections (`%description`, `%prep`, `%files`, `%changelog`, …).
//!
//! Build-script sections (`%prep`, `%build`, `%install`, `%check`, …) and
//! scriptlet/trigger bodies are stored as [`ShellBody`] — sequences of text
//! lines that may contain macro references but are *not* parsed as bash.

#![allow(missing_docs)]

use super::changelog::ChangelogItem;
use super::cond::{CondExpr, CondKind};
use super::files::FilesContent;
use super::preamble::PreambleContent;
use super::scriptlet::{FileTrigger, Scriptlet, Trigger};
use super::text::Text;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum Section<T = ()> {
    /// `%description [-n] [sub]` — free-form text body.
    Description {
        subpkg: Option<SubpkgRef>,
        body: TextBody,
        data: T,
    },
    /// `%package [-n] sub` — declares a subpackage with its own preamble.
    Package {
        name_arg: PackageName,
        content: Vec<PreambleContent<T>>,
        data: T,
    },
    /// `%prep` / `%conf` / `%build` / `%install` / `%check` / `%clean` /
    /// `%generate_buildrequires` — shell bodies.
    BuildScript {
        kind: BuildScriptKind,
        /// Placement from the build-script section header.
        #[cfg_attr(feature = "serde", serde(default))]
        placement: BuildScriptPlacement,
        body: ShellBody<T>,
        data: T,
    },
    /// `%files [-n sub] [-f filelist]` — a list of paths plus directives.
    Files {
        subpkg: Option<SubpkgRef>,
        /// `-f filelist` — repeatable. Each entry is a path (often a macro).
        file_lists: Vec<Text>,
        content: Vec<FilesContent<T>>,
        data: T,
    },
    Scriptlet(Scriptlet<T>),
    Trigger(Trigger<T>),
    FileTrigger(FileTrigger<T>),
    /// `%verify [-n sub]` — shell body executed by `rpm --verify`.
    Verify {
        subpkg: Option<SubpkgRef>,
        body: ShellBody<T>,
        data: T,
    },
    /// `%changelog` — the single global changelog block.
    Changelog {
        items: Vec<ChangelogItem<T>>,
        data: T,
    },
    /// `%sourcelist` — alternative to numbered `SourceN:` tags.
    SourceList {
        entries: Vec<Text>,
        data: T,
    },
    /// `%patchlist` — alternative to numbered `PatchN:` tags.
    PatchList {
        entries: Vec<Text>,
        data: T,
    },
    /// `%sepolicy [-n sub]` — SELinux module section (RH family).
    Sepolicy {
        subpkg: Option<SubpkgRef>,
        body: ShellBody<T>,
        data: T,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum BuildScriptKind {
    Prep,
    /// `%conf` — rpm ≥ 4.18.
    Conf,
    Build,
    Install,
    Check,
    /// `%clean` — deprecated; rpm cleans the buildroot itself.
    Clean,
    /// `%generate_buildrequires` — rpm ≥ 4.15.
    GenerateBuildRequires,
}

/// Source-level placement of a build-script section fragment.
///
/// RPM 4.20 and later use `-p` and `-a` to augment the main section.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum BuildScriptPlacement {
    /// The main section has no placement flag.
    #[default]
    Main,
    /// `-p` prepends the section fragment.
    Prepend,
    /// `-a` appends the section fragment.
    Append,
}

/// Argument of `%package`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum PackageName {
    /// `%package foo` — appended to the main name: `<main>-foo`.
    Relative(Text),
    /// `%package -n foo` — absolute subpackage name `foo`.
    Absolute(Text),
}

/// `-n SUB` or bare `SUB` modifier on a section header.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum SubpkgRef {
    /// Bare suffix form: `<main>-SUB`.
    Relative(Text),
    /// `-n` form: absolute subpackage name.
    Absolute(Text),
}

/// Free-form text body (e.g. inside `%description`). Each entry is one line;
/// an empty [`Text`] denotes a blank line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct TextBody {
    pub lines: Vec<Text>,
}

/// Shell body (e.g. inside `%prep`, `%build`, scriptlets). This crate does
/// *not* parse the bash grammar, but it does distinguish literal text from
/// embedded macro references so they can be expanded or printed cleanly.
///
/// Conditional blocks (`%if`/`%else`/`%endif`) appearing inside a shell body
/// remain present in [`Self::lines`] as plain `Text` (one entry per physical
/// line, including the directive lines themselves) so existing consumers
/// that walk `lines` keep working unchanged. The same blocks are *additionally*
/// surfaced as structural [`ShellConditional`] entries in [`Self::conditionals`],
/// enabling branch-aware analyses (e.g. `matrix expand`) without parsing the
/// shell-body text twice.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[non_exhaustive]
pub struct ShellBody<T = ()> {
    pub lines: Vec<Text>,
    /// Conditional `%if`/`%else`/`%endif` blocks recognised inside the body,
    /// in source order. Empty when the source has no conditionals;
    /// `#[serde(default)]` keeps backward compatibility with serialised forms
    /// that pre-date this field.
    #[cfg_attr(
        feature = "serde",
        serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")
    )]
    pub conditionals: Vec<ShellConditional<T>>,
}

// Manual `Default` impl: we want `ShellBody::<T>::default()` for any `T`,
// without forcing `T: Default` (the derived form would; that bound then
// propagates to every `Scriptlet<T>` / `Trigger<T>` / `FileTrigger<T>` that
// embeds a `ShellBody<T>`).
impl<T> Default for ShellBody<T> {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            conditionals: Vec::new(),
        }
    }
}

/// One `%if`...`%endif` block detected inside a [`ShellBody`].
///
/// Carries the parsed expression(s) of each branch plus span/line info so a
/// consumer can map back into [`ShellBody::lines`]. The branch *bodies* are
/// not duplicated here — analyzers wanting the body content read it from
/// `lines` using [`Self::data`] (or the branch's `data`/`head_line`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct ShellConditional<T = ()> {
    /// `%if` / `%ifarch` / ... head plus any `%elif*` clauses, in source order.
    pub branches: Vec<ShellCondBranch<T>>,
    /// `%else` clause, if any.
    pub otherwise: Option<ShellCondElse<T>>,
    /// Per-node user-data (typically a [`super::span::Span`] over the whole block, from
    /// `%if`* head to closing `%endif`).
    pub data: T,
}

impl<T> ShellConditional<T> {
    /// Create a new shell-body conditional block.
    #[must_use]
    pub fn new(
        branches: Vec<ShellCondBranch<T>>,
        otherwise: Option<ShellCondElse<T>>,
        data: T,
    ) -> Self {
        Self {
            branches,
            otherwise,
            data,
        }
    }
}

/// One branch (`%if` / `%elif*`) of a [`ShellConditional`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct ShellCondBranch<T = ()> {
    pub kind: CondKind,
    pub expr: CondExpr<T>,
    /// Per-node user-data (typically a [`super::span::Span`] covering this branch from its
    /// directive line through to the start of the next sibling or the
    /// closing `%endif`).
    pub data: T,
    /// 1-based source line number of the directive line itself.
    pub head_line: u32,
}

impl<T> ShellCondBranch<T> {
    /// Create a new `%if`/`%elif*` branch entry.
    #[must_use]
    pub fn new(kind: CondKind, expr: CondExpr<T>, data: T, head_line: u32) -> Self {
        Self {
            kind,
            expr,
            data,
            head_line,
        }
    }
}

/// `%else` clause of a [`ShellConditional`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct ShellCondElse<T = ()> {
    /// Per-node user-data (typically a [`super::span::Span`] over the `%else` directive
    /// line and its body).
    pub data: T,
    /// 1-based source line number of the `%else` directive line.
    pub head_line: u32,
}

impl<T> ShellCondElse<T> {
    /// Create a new `%else` clause entry.
    #[must_use]
    pub fn new(data: T, head_line: u32) -> Self {
        Self { data, head_line }
    }
}
