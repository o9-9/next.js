use anyhow::{Result, bail};
use indoc::formatdoc;
use turbo_rcstr::RcStr;
use turbo_tasks::{FxIndexMap, ResolvedVc, Vc};
use turbo_tasks_fs::FileSystemPath;
use turbopack::{ModuleAssetContext, transition::Transition};
use turbopack_core::{
    boundary::BoundaryInfo,
    context::ProcessResult,
    file_source::FileSource,
    module::Module,
    reference_type::{EcmaScriptModulesReferenceSubType, ReferenceType},
    source::Source,
};
use turbopack_ecmascript::{magic_identifier, utils::StringifyJs};

pub struct BaseLoaderTreeBuilder {
    pub inner_assets: FxIndexMap<RcStr, ResolvedVc<Box<dyn Module>>>,
    pub inner_asset_boundaries: FxIndexMap<RcStr, ResolvedVc<Box<dyn BoundaryInfo>>>,
    counter: usize,
    pub imports: Vec<RcStr>,
    pub module_asset_context: ResolvedVc<ModuleAssetContext>,
    pub server_component_transition: ResolvedVc<Box<dyn Transition>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppDirModuleType {
    Page,
    DefaultPage,
    Error,
    Layout,
    Loading,
    Template,
    NotFound,
    Forbidden,
    Unauthorized,
    GlobalError,
    GlobalNotFound,
}

impl AppDirModuleType {
    pub fn name(&self) -> &'static str {
        match self {
            AppDirModuleType::Page => "page",
            AppDirModuleType::DefaultPage => "defaultPage",
            AppDirModuleType::Error => "error",
            AppDirModuleType::Layout => "layout",
            AppDirModuleType::Loading => "loading",
            AppDirModuleType::Template => "template",
            AppDirModuleType::NotFound => "not-found",
            AppDirModuleType::Forbidden => "forbidden",
            AppDirModuleType::Unauthorized => "unauthorized",
            AppDirModuleType::GlobalError => "global-error",
            AppDirModuleType::GlobalNotFound => "global-not-found",
        }
    }
}

impl BaseLoaderTreeBuilder {
    pub fn new(
        module_asset_context: ResolvedVc<ModuleAssetContext>,
        server_component_transition: ResolvedVc<Box<dyn Transition>>,
    ) -> Self {
        BaseLoaderTreeBuilder {
            inner_assets: FxIndexMap::default(),
            inner_asset_boundaries: FxIndexMap::default(),
            counter: 0,
            imports: Vec::new(),
            module_asset_context,
            server_component_transition,
        }
    }

    pub fn unique_number(&mut self) -> usize {
        let i = self.counter;
        self.counter += 1;
        i
    }

    pub async fn process_source(
        &self,
        source: Vc<Box<dyn Source>>,
    ) -> Result<(
        ResolvedVc<Box<dyn Module>>,
        Option<ResolvedVc<Box<dyn BoundaryInfo>>>,
    )> {
        let reference_type =
            ReferenceType::EcmaScriptModules(EcmaScriptModulesReferenceSubType::Undefined);

        let process_result = self
            .server_component_transition
            .process(source, *self.module_asset_context, reference_type)
            .await?;
        match &*process_result {
            ProcessResult::Module { module, boundary } => Ok((*module, *boundary)),
            ProcessResult::Ignore => {
                bail!("Expected process result to be a module, but it was ignored")
            }
            ProcessResult::Unknown(_) => {
                bail!("Expected process result to be a module, but it could not be processed")
            }
        }
    }

    pub fn process_module(&self, module: Vc<Box<dyn Module>>) -> Vc<Box<dyn Module>> {
        self.server_component_transition
            .process_module(module, *self.module_asset_context)
    }

    pub async fn create_module_tuple_code(
        &mut self,
        module_type: AppDirModuleType,
        path: FileSystemPath,
    ) -> Result<String> {
        let name = module_type.name();
        let i = self.unique_number();
        let identifier = magic_identifier::mangle(&format!("{name} #{i}"));

        self.imports.push(
            formatdoc!(
                r#"
                const {} = () => require("MODULE_{}");
                "#,
                identifier,
                i
            )
            .into(),
        );

        let (module, boundary) = self
            .process_source(Vc::upcast(FileSource::new(path.clone())))
            .await?;

        let key: RcStr = format!("MODULE_{i}").into();
        self.inner_assets.insert(key.clone(), module);
        if let Some(boundary) = boundary {
            self.inner_asset_boundaries.insert(key, boundary);
        }

        // Use the original source path, not the transformed module path.
        // This is important for MDX files where page.mdx becomes page.mdx.tsx after
        // transformation, but the font manifest uses the original source path.
        let module_path = path.value_to_string().await?;

        Ok(format!(
            "[{identifier}, {path}]",
            path = StringifyJs(&module_path),
        ))
    }
}
