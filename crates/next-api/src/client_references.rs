use anyhow::Result;
use bincode::{Decode, Encode};
use next_core::boundary_types::{
    CssClientReferenceBoundary, EcmascriptClientReferenceBoundary, boundary_type_server_component,
    boundary_type_server_utility,
};
use rustc_hash::FxHashMap;
use turbo_tasks::{
    NonLocalValue, ResolvedVc, TryFlatJoinIterExt, Vc, debug::ValueDebugFormat, trace::TraceRawVcs,
};
use turbopack_core::{boundary::BoundaryInfo, module::Module, module_graph::ModuleGraphLayer};
use turbopack_css::chunk::CssChunkPlaceable;
use turbopack_ecmascript::chunk::EcmascriptChunkPlaceable;

#[derive(
    Copy, Clone, Eq, PartialEq, TraceRawVcs, ValueDebugFormat, NonLocalValue, Encode, Decode,
)]
pub enum ClientManifestEntryType {
    EcmascriptClientReference {
        client_module: ResolvedVc<Box<dyn EcmascriptChunkPlaceable>>,
        ssr_module: ResolvedVc<Box<dyn EcmascriptChunkPlaceable>>,
    },
    CssClientReference(ResolvedVc<Box<dyn CssChunkPlaceable>>),
    /// Server component or server utility boundary (boundary_type cached for sync access)
    Boundary {
        boundary: ResolvedVc<Box<dyn BoundaryInfo>>,
        is_server_component: bool,
    },
}

/// Tracks information about all the css and js client references in the graph.
#[turbo_tasks::value(transparent)]
pub struct ClientReferenceData(FxHashMap<ResolvedVc<Box<dyn Module>>, ClientManifestEntryType>);

#[turbo_tasks::function]
pub async fn map_client_references(
    graph: ResolvedVc<ModuleGraphLayer>,
) -> Result<Vc<ClientReferenceData>> {
    let graph = graph.await?;
    let manifest = graph
        .iter_nodes_with_boundary()
        .map(|(module, boundary)| async move {
            // All detection is now boundary-based
            let Some(boundary) = boundary else {
                return Ok(None);
            };

            let boundary_type = boundary.boundary_type().await?;

            if *boundary_type == boundary_type_server_component() {
                return Ok(Some((
                    module,
                    ClientManifestEntryType::Boundary {
                        boundary,
                        is_server_component: true,
                    },
                )));
            }

            if *boundary_type == boundary_type_server_utility() {
                return Ok(Some((
                    module,
                    ClientManifestEntryType::Boundary {
                        boundary,
                        is_server_component: false,
                    },
                )));
            }

            // Client references - downcast boundary to get module-specific data
            if let Some(ecma_boundary) =
                ResolvedVc::try_downcast_type::<EcmascriptClientReferenceBoundary>(boundary)
            {
                let boundary_data = ecma_boundary.await?;
                return Ok(Some((
                    module,
                    ClientManifestEntryType::EcmascriptClientReference {
                        client_module: boundary_data.client_module,
                        ssr_module: boundary_data.ssr_module,
                    },
                )));
            }

            if let Some(css_boundary) =
                ResolvedVc::try_downcast_type::<CssClientReferenceBoundary>(boundary)
            {
                let boundary_data = css_boundary.await?;
                return Ok(Some((
                    module,
                    ClientManifestEntryType::CssClientReference(boundary_data.client_module),
                )));
            }

            Ok(None)
        })
        .try_flat_join()
        .await?
        .into_iter()
        .collect::<FxHashMap<_, _>>();

    Ok(Vc::cell(manifest))
}
