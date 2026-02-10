//! Boundary type constants for Next.js module boundaries.
//!
//! These are used with the `boundary` field on `ProcessResult::Module` and
//! `ModuleResolveResultItem::Module` to mark modules that cross framework
//! boundaries (e.g., server components, client references).

use once_cell::sync::Lazy;
use turbo_rcstr::{RcStr, rcstr};
use turbo_tasks::{ResolvedVc, Vc};
use turbopack_core::boundary::{BoundaryInfo, OptionFileSystemPath};
use turbopack_css::chunk::CssChunkPlaceable;
use turbopack_ecmascript::chunk::EcmascriptChunkPlaceable;

/// Server component boundary - marks modules that are React Server Components.
pub fn boundary_type_server_component() -> RcStr {
    rcstr!("server-component")
}

/// Server utility boundary - marks modules decorated with 'use server'.
pub fn boundary_type_server_utility() -> RcStr {
    rcstr!("server-utility")
}

/// Dynamic import entry - marks modules loaded via next/dynamic.
pub fn boundary_type_dynamic_entry() -> RcStr {
    rcstr!("dynamic-entry")
}

/// Ecmascript client reference - marks client components referenced from server components.
pub fn boundary_type_client_reference() -> RcStr {
    rcstr!("client-reference")
}

/// CSS client reference - marks CSS modules referenced from server components.
pub fn boundary_type_css_client_reference() -> RcStr {
    rcstr!("css-client-reference")
}

/// Merge tag for server utility chunk groups.
pub static SERVER_UTILITY_MERGE_TAG: Lazy<RcStr> = Lazy::new(|| rcstr!("next-server-utility"));

/// Boundary info for CSS client references, carrying the client module.
#[turbo_tasks::value(shared)]
#[derive(Clone, Debug)]
pub struct CssClientReferenceBoundary {
    pub client_module: ResolvedVc<Box<dyn CssChunkPlaceable>>,
}

impl CssClientReferenceBoundary {
    pub fn new(client_module: ResolvedVc<Box<dyn CssChunkPlaceable>>) -> Self {
        Self { client_module }
    }
}

#[turbo_tasks::value_impl]
impl BoundaryInfo for CssClientReferenceBoundary {
    #[turbo_tasks::function]
    fn boundary_type(&self) -> Vc<RcStr> {
        Vc::cell(boundary_type_css_client_reference())
    }

    #[turbo_tasks::function]
    fn source_path(&self) -> Vc<OptionFileSystemPath> {
        Vc::cell(None)
    }
}

/// Boundary info for Ecmascript client references, carrying client and SSR modules.
#[turbo_tasks::value(shared)]
#[derive(Clone, Debug)]
pub struct EcmascriptClientReferenceBoundary {
    pub client_module: ResolvedVc<Box<dyn EcmascriptChunkPlaceable>>,
    pub ssr_module: ResolvedVc<Box<dyn EcmascriptChunkPlaceable>>,
}

impl EcmascriptClientReferenceBoundary {
    pub fn new(
        client_module: ResolvedVc<Box<dyn EcmascriptChunkPlaceable>>,
        ssr_module: ResolvedVc<Box<dyn EcmascriptChunkPlaceable>>,
    ) -> Self {
        Self {
            client_module,
            ssr_module,
        }
    }
}

#[turbo_tasks::value_impl]
impl BoundaryInfo for EcmascriptClientReferenceBoundary {
    #[turbo_tasks::function]
    fn boundary_type(&self) -> Vc<RcStr> {
        Vc::cell(boundary_type_client_reference())
    }

    #[turbo_tasks::function]
    fn source_path(&self) -> Vc<OptionFileSystemPath> {
        Vc::cell(None)
    }
}
