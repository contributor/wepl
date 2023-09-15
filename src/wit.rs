use std::borrow::Cow;

use anyhow::Context;
use wit_component::DecodedWasm;
use wit_parser::{Function, InterfaceId, Resolve, TypeDef, World, WorldId, WorldItem, WorldKey};

pub struct Querier {
    resolve: Resolve,
    world_id: WorldId,
}

impl Querier {
    /// Create new instance.
    ///
    /// Panics if the `world_id` is not found in the `resolve`.
    pub fn new(resolve: Resolve, world_id: WorldId) -> Self {
        let this = Self { resolve, world_id };
        // Ensure the world can be resolved
        let _ = this.world();
        this
    }

    pub fn from_bytes(component_bytes: &[u8]) -> anyhow::Result<Self> {
        let (resolve, world) = match wit_component::decode(component_bytes)
            .context("could not decode given file as a WebAssembly component")?
        {
            DecodedWasm::Component(r, w) => (r, w),
            _ => anyhow::bail!("found wit package instead of the expect WebAssembly component"),
        };
        Ok(Self::new(resolve, world))
    }

    pub fn exported_function(&self, name: &str) -> Option<&Function> {
        let export = self.export(name)?;
        match export {
            wit_parser::WorldItem::Function(f) => Some(f),
            _ => None,
        }
    }

    pub fn imported_function(&self, name: &str) -> Option<&Function> {
        let export = self.import(name)?;
        match export {
            wit_parser::WorldItem::Function(f) => Some(f),
            _ => None,
        }
    }

    pub fn export(&self, name: &str) -> Option<&WorldItem> {
        self.get_world_item_by_name(self.world().exports.iter(), name)
    }

    pub fn import(&self, name: &str) -> Option<&WorldItem> {
        self.get_world_item_by_name(self.world().imports.iter(), name)
    }

    pub fn interface(&self, id: InterfaceId) -> Option<&wit_parser::Interface> {
        self.resolve.interfaces.get(id)
    }

    pub(crate) fn types_by_name(&self, name: &str) -> Vec<(Option<&InterfaceId>, &TypeDef)> {
        let mut types = Vec::new();
        for (_, t) in &self.resolve.types {
            if t.name.as_deref().map(|n| n == name).unwrap_or_default() {
                let interface = if let wit_parser::TypeOwner::Interface(i) = &t.owner {
                    Some(i)
                } else {
                    None
                };
                types.push((interface, t));
            }
        }
        types
    }

    pub fn display_wit_type<'a>(
        &'a self,
        param_type: &wit_parser::Type,
        expansion: Expansion,
    ) -> Cow<'a, str> {
        let str = match param_type {
            wit_parser::Type::Bool => "bool",
            wit_parser::Type::U8 => "u8",
            wit_parser::Type::U16 => "u16",
            wit_parser::Type::U32 => "u32",
            wit_parser::Type::U64 => "u64",
            wit_parser::Type::S8 => "s8",
            wit_parser::Type::S16 => "s16",
            wit_parser::Type::S32 => "s32",
            wit_parser::Type::S64 => "s64",
            wit_parser::Type::Float32 => "float32",
            wit_parser::Type::Float64 => "float64",
            wit_parser::Type::String => "string",
            wit_parser::Type::Char => "char",
            wit_parser::Type::Id(id) => {
                let typ = self
                    .resolve
                    .types
                    .get(*id)
                    .expect("found type id for type not present in resolver");
                return self.display_wit_type_def(typ, expansion);
            }
        };
        Cow::Borrowed(str)
    }

    pub fn display_wit_type_def(&self, typ: &TypeDef, expansion: Expansion) -> Cow<'_, str> {
        let display = match &typ.kind {
            wit_parser::TypeDefKind::Option(o) => {
                format!("option<{}>", self.display_wit_type(o, Expansion::Collapsed))
            }
            wit_parser::TypeDefKind::Result(r) => {
                let ok =
                    r.ok.as_ref()
                        .map(|o| self.display_wit_type(o, Expansion::Collapsed));
                let err = r
                    .err
                    .as_ref()
                    .map(|o| self.display_wit_type(o, Expansion::Collapsed));
                match (ok, err) {
                    (Some(ok), Some(err)) => format!("result<{ok}, {err}>"),
                    (Some(t), _) | (_, Some(t)) => format!("result<{t}>"),
                    _ => "result".into(),
                }
            }
            wit_parser::TypeDefKind::Type(t) => return self.display_wit_type(t, expansion),
            wit_parser::TypeDefKind::List(t) => {
                format!("list<{}>", self.display_wit_type(t, Expansion::Collapsed))
            }
            wit_parser::TypeDefKind::Tuple(t) => {
                format!(
                    "tuple<{}>",
                    t.types
                        .iter()
                        .map(|t| self.display_wit_type(t, Expansion::Collapsed))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            wit_parser::TypeDefKind::Enum(e) => match expansion {
                Expansion::Expanded(col) => {
                    let fields = e
                        .cases
                        .iter()
                        .map(|c| format!("{}{}", " ".repeat(col as usize * 4), c.name))
                        .collect::<Vec<_>>()
                        .join(",\n");
                    format!(
                        "enum {{\n{fields}\n{}}}",
                        " ".repeat((col - 1) as usize * 4)
                    )
                }
                Expansion::Collapsed => typ.name.clone().unwrap(),
            },
            wit_parser::TypeDefKind::Record(r) => match expansion {
                Expansion::Expanded(col) => {
                    let fields = r
                        .fields
                        .iter()
                        .map(|f| {
                            format!(
                                "{}: {}",
                                f.name,
                                self.display_wit_type(&f.ty, Expansion::Collapsed)
                            )
                        })
                        .map(|f| format!("{}{}", " ".repeat(col as usize * 4), f))
                        .collect::<Vec<_>>()
                        .join(",\n");
                    format!(
                        "record {{\n{fields}\n{}}}",
                        " ".repeat((col - 1) as usize * 4)
                    )
                }
                Expansion::Collapsed => typ.name.clone().unwrap(),
            },
            wit_parser::TypeDefKind::Resource => todo!(),
            wit_parser::TypeDefKind::Handle(_) => todo!(),
            wit_parser::TypeDefKind::Flags(_) => todo!(),
            wit_parser::TypeDefKind::Variant(_) => todo!(),
            wit_parser::TypeDefKind::Future(_) => todo!(),
            wit_parser::TypeDefKind::Stream(_) => todo!(),
            wit_parser::TypeDefKind::Unknown => unreachable!(),
        };
        Cow::Owned(display)
    }

    pub fn imports_wasi(&self) -> bool {
        let world = self.world();
        for (import_name, _) in &world.imports {
            if let WorldKey::Interface(interface_id) = import_name {
                let interface = self.resolve.interfaces.get(*interface_id).unwrap();
                if let Some(package_id) = &interface.package {
                    if let Some(package) = self.resolve.packages.get(*package_id) {
                        if package.name.namespace == "wasi" {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    pub fn non_wasi_imports(&self) -> impl Iterator<Item = (String, &WorldItem)> {
        self.world()
            .imports
            .iter()
            .filter_map(|(import_name, import)| {
                if let WorldKey::Interface(i) = import_name {
                    let interface = self.resolve.interfaces.get(*i).unwrap();
                    if let Some(package_id) = &interface.package {
                        let package = self.resolve.packages.get(*package_id).unwrap();
                        if package.name.namespace == "wasi" {
                            return None;
                        }
                    }
                }
                let import_name = self.resolve.name_world_key(import_name);
                Some((import_name, import))
            })
    }

    pub fn world_item_name(&self, name: &WorldKey) -> String {
        self.resolve.name_world_key(name)
    }

    pub fn interface_name(&self, interface: &InterfaceId) -> Option<String> {
        self.resolve.id_of(*interface)
    }

    pub fn world(&self) -> &World {
        self.resolve
            .worlds
            .get(self.world_id)
            .expect("world_id is not found in the resolved wit package")
    }

    pub(crate) fn check_dynamic_import(
        &self,
        func_name: &str,
        component_bytes: &[u8],
    ) -> anyhow::Result<()> {
        let other = Self::from_bytes(component_bytes)?;
        let import = self
            .imported_function(func_name)
            .with_context(|| format!("no import with name '{func_name}'"))?;
        let export = other
            .exported_function(func_name)
            .with_context(|| format!("no export with name '{func_name}'"))?;
        if import.params != export.params {
            anyhow::bail!("params not equal")
        }
        if import.results != export.results {
            anyhow::bail!("return values not equal")
        }

        Ok(())
    }
    fn get_world_item_by_name<'a>(
        &self,
        mut items: impl Iterator<Item = (&'a WorldKey, &'a WorldItem)>,
        name: &str,
    ) -> Option<&'a WorldItem> {
        items.find_map(|(export_name, export)| {
            let export_name = self.resolve.name_world_key(export_name);
            (export_name == name).then_some(export)
        })
    }

    pub(crate) fn imports(
        &self,
        include_wasi: bool,
    ) -> impl Iterator<Item = (&WorldKey, &WorldItem)> {
        self.world()
            .imports
            .iter()
            .filter(move |(_, item)| match item {
                WorldItem::Interface(id) if !include_wasi => {
                    let interface = self.interface(*id).unwrap();
                    let Some(package) = interface.package else {
                        return true;
                    };
                    let package = self.resolve.packages.get(package).unwrap();
                    package.name.namespace != "wasi"
                }
                _ => true,
            })
    }
}

pub enum Expansion {
    Expanded(u8),
    Collapsed,
}
