//! Module boundary types for tracking boundary crossings in the module graph.
//!
//! Boundaries represent transitions between different module contexts (e.g., server components,
//! client references). Instead of using wrapper modules to mark these boundaries, the boundary
//! information is attached to the resolution result directly via an optional `boundary` field
//! in `ProcessResult::Module` and `ModuleResolveResultItem::Module`.
//!
//! The boundary type is an RcStr to keep turbopack generic - specific boundary type constants
//! should be defined in the framework-specific crates (e.g., next-core).

use turbo_rcstr::RcStr;
use turbo_tasks::{ResolvedVc, Vc};
use turbo_tasks_fs::FileSystemPath;

use crate::chunk::ChunkingType;

/// Optional file system path wrapper for use with Vc.
#[turbo_tasks::value(transparent)]
pub struct OptionFileSystemPath(Option<FileSystemPath>);

/// Optional chunking type wrapper for use with Vc.
#[turbo_tasks::value(transparent)]
pub struct OptionChunkingType(Option<ChunkingType>);

/// Optional boundary info wrapper for use with Vc.
#[turbo_tasks::value(transparent)]
pub struct OptionBoundaryInfo(Option<ResolvedVc<Box<dyn BoundaryInfo>>>);

/// Trait for boundary metadata associated with a module crossing.
///
/// This is stored in the optional `boundary` field of `ProcessResult::Module` and
/// `ModuleResolveResultItem::Module` to indicate that a module crossing represents
/// a boundary without wrapping the module in a marker type.
///
/// Implementations can carry additional data specific to the boundary type,
/// which can be accessed by downcasting the boundary.
#[turbo_tasks::value_trait]
pub trait BoundaryInfo {
    /// The type of boundary being crossed (e.g., "server-component", "client-reference").
    /// This is an RcStr to keep turbopack generic - specific values are defined in
    /// framework-specific crates.
    #[turbo_tasks::function]
    fn boundary_type(self: Vc<Self>) -> Vc<RcStr>;

    /// Original source path before any transformations (e.g., before MDX -> MDX.tsx).
    /// Used for server components where the manifest key needs the original path.
    #[turbo_tasks::function]
    fn source_path(self: Vc<Self>) -> Vc<OptionFileSystemPath> {
        Vc::cell(None)
    }

    /// Optional chunking type override for this boundary.
    /// When set, this chunking type is applied to modules that cross this boundary,
    /// replacing the reference's chunking type. This is used to preserve the chunking
    /// semantics that were previously provided by wrapper modules.
    #[turbo_tasks::function]
    fn chunking_type(self: Vc<Self>) -> Vc<OptionChunkingType> {
        Vc::cell(None)
    }
}

/// Simple boundary info for boundaries that only need a type and optional source path.
/// This is used for trivial boundaries like server-component and server-utility.
#[turbo_tasks::value(shared)]
#[derive(Clone, Debug)]
pub struct SimpleBoundary {
    pub boundary_type: RcStr,
    pub source_path: Option<FileSystemPath>,
    pub chunking_type: Option<ChunkingType>,
}

impl SimpleBoundary {
    /// Create a new simple boundary with the given type and no source path
    pub fn new(boundary_type: RcStr) -> Self {
        Self {
            boundary_type,
            source_path: None,
            chunking_type: None,
        }
    }

    /// Create a new simple boundary with the given type and source path
    pub fn with_source_path(boundary_type: RcStr, source_path: FileSystemPath) -> Self {
        Self {
            boundary_type,
            source_path: Some(source_path),
            chunking_type: None,
        }
    }

    /// Create a new simple boundary with the given type, source path, and chunking type
    pub fn with_source_path_and_chunking(
        boundary_type: RcStr,
        source_path: FileSystemPath,
        chunking_type: ChunkingType,
    ) -> Self {
        Self {
            boundary_type,
            source_path: Some(source_path),
            chunking_type: Some(chunking_type),
        }
    }

    /// Create a new simple boundary with the given type and chunking type (no source path)
    pub fn with_chunking(boundary_type: RcStr, chunking_type: ChunkingType) -> Self {
        Self {
            boundary_type,
            source_path: None,
            chunking_type: Some(chunking_type),
        }
    }
}

#[turbo_tasks::value_impl]
impl BoundaryInfo for SimpleBoundary {
    #[turbo_tasks::function]
    fn boundary_type(&self) -> Vc<RcStr> {
        Vc::cell(self.boundary_type.clone())
    }

    #[turbo_tasks::function]
    fn source_path(&self) -> Vc<OptionFileSystemPath> {
        Vc::cell(self.source_path.clone())
    }

    #[turbo_tasks::function]
    fn chunking_type(&self) -> Vc<OptionChunkingType> {
        Vc::cell(self.chunking_type.clone())
    }
}
