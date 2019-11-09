use std::fmt::{Display, Formatter, Write};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use codegen::Scope;
use heck::SnakeCase;

use crate::bond::*;
use crate::Result;

trait Visitor<T> {
    type Result;

    fn visit(&self, item: &T) -> Self::Result;
}

pub struct Compiler;

impl Compiler {
    pub fn new() -> Self {
        Self
    }

    pub fn compile_all(&self, input_dir: &Path, output_dir: &Path) -> Result<()> {
        let mut files: Vec<_> = fs::read_dir(&input_dir)?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|file| 1 == 1 || file.ends_with("Envelope.json"))
            .collect();
        files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        self.compile_files(output_dir, files.iter())?;
        self.compile_package(output_dir, files.iter())?;

        Ok(())
    }

    fn compile_files<'a>(&self, output_dir: &'a Path, files: impl Iterator<Item = &'a PathBuf>) -> Result<()> {
        for input in files {
            let stem = input
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| stem.to_lowercase())
                .ok_or("Unable to get a file name")?;

            let output = output_dir.join(format!("{}.rs", stem));

            let file_name = input
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("Unable to get a file name")?;

            if let Err(err) = self.compile(&input, &output) {
                eprintln!("{}: {}", file_name, err);
            } else {
                println!("{}: ok", file_name);
            }
        }

        Ok(())
    }

    pub fn compile(&self, input: &Path, output: &Path) -> Result<()> {
        let parser = Parser::new();
        let schema = parser.parse(input)?;

        let generator = CodeGenerator;
        let module = generator.visit(&schema);

        fs::write(output, module.to_string())?;
        Ok(())
    }

    fn compile_package<'a>(&self, output_dir: &'a Path, files: impl Iterator<Item = &'a PathBuf>) -> Result<()> {
        let module_names: Vec<_> = files
            .into_iter()
            .filter_map(|file| {
                file.file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(|stem| stem.to_lowercase())
            })
            .collect();

        let modules_block = module_names.iter().fold(String::new(), |mut block, name| {
            writeln!(block, "mod {};", name).unwrap();
            block
        });

        let use_block = module_names.iter().fold(String::new(), |mut block, name| {
            writeln!(block, "pub use {}::*;", name).unwrap();
            block
        });

        let mut package = codegen::Scope::new();
        package
            .raw("// NOTE: This file was automatically generated.")
            .raw("#![allow(unused_variables, dead_code)]")
            .raw(&modules_block)
            .raw(&use_block);

        let package_path = output_dir.join("mod.rs");
        fs::write(&package_path, package.to_string())?;

        Ok(())
    }
}

struct CodeGenerator;

impl Visitor<Schema> for CodeGenerator {
    type Result = Scope;

    fn visit(&self, item: &Schema) -> Self::Result {
        let mut module = Scope::new();
        module.raw("// NOTE: This file was automatically generated.");

        for declaration in item.declarations.iter() {
            match declaration {
                UserTypeDeclaration::Struct(struct_) => {
                    let (struct_, impl_) = self.visit(struct_);
                    module.push_struct(struct_);
                    module.push_impl(impl_);
                }
                UserTypeDeclaration::Enum(enum_) => {
                    let enum_ = self.visit(enum_);
                    module.push_enum(enum_);
                }
            };
        }

        module
    }
}

struct StructCodeGenerator;

impl Visitor<Struct> for StructCodeGenerator {
    type Result = codegen::Struct;

    fn visit(&self, item: &Struct) -> Self::Result {
        let mut struct_: codegen::Struct = codegen::Struct::new(&item.decl_name);
        struct_.vis("pub");

        for field in item.struct_fields.to_vec() {
            if let Some(generic) = field.field_type.generic() {
                struct_.generic(generic);
            }

            let field_name = FieldName::from(&field.field_name);
            let field_type = codegen::Type::from(field);
            struct_.field(field_name.as_ref(), &field_type);
        }

        if let Some(doc) = Doc.visit(&item.decl_attributes) {
            struct_.doc(&doc);
        }

        struct_.derive("Debug");

        struct_
    }
}

struct ConstructorCodeGenerator;

impl Visitor<Struct> for ConstructorCodeGenerator {
    type Result = codegen::Function;

    fn visit(&self, item: &Struct) -> Self::Result {
        let mut block = codegen::Block::new("Self");
        let mut constructor = codegen::Function::new("new");
        constructor.vis("pub");
        constructor.ret("Self");

        for field in item.struct_fields.to_vec() {
            let field_name = FieldName::from(&field.field_name);
            let field_type = codegen::Type::from(field);

            constructor.arg(field_name.as_ref(), &field_type);
            block.line(format!("{},", field_name));
        }

        constructor.push_block(block);

        constructor
    }
}

struct ImplCodeGenerator;

impl Visitor<Struct> for ImplCodeGenerator {
    type Result = codegen::Impl;

    fn visit(&self, item: &Struct) -> Self::Result {
        let generics: Vec<_> = item
            .struct_fields
            .iter()
            .filter_map(|field| field.field_type.generic())
            .collect();

        let type_ = generics
            .iter()
            .fold(codegen::Type::from(&item.decl_name), |mut type_, generic| {
                type_.generic(*generic);
                type_
            });

        generics.iter().fold(codegen::Impl::new(&type_), |mut impl_, generic| {
            impl_.generic(*generic);
            impl_
        })
    }
}

impl Visitor<Struct> for CodeGenerator {
    type Result = (codegen::Struct, codegen::Impl);

    fn visit(&self, item: &Struct) -> Self::Result {
        let struct_ = StructCodeGenerator.visit(&item);
        let mut impl_ = ImplCodeGenerator.visit(&item);
        impl_.push_fn(ConstructorCodeGenerator.visit(&item));

        (struct_, impl_)
    }
}

impl Visitor<Enum> for CodeGenerator {
    type Result = codegen::Enum;

    fn visit(&self, item: &Enum) -> Self::Result {
        let mut enum_ = codegen::Enum::new(&item.decl_name);
        enum_.vis("pub");

        for const_ in item.enum_constants.iter() {
            enum_.new_variant(&const_.constant_name);

            if let Some(_) = &const_.constant_value {
                panic!("enum value is not supported: {:#?}", const_)
            }
        }

        if let Some(doc) = Doc.visit(&item.decl_attributes) {
            enum_.doc(&doc);
        }

        enum_.derive("Debug");

        enum_
    }
}

struct Doc;

impl Visitor<Vec<Attribute>> for Doc {
    type Result = Option<String>;

    fn visit(&self, items: &Vec<Attribute>) -> Self::Result {
        items.into_iter().filter_map(|attr| self.visit(attr)).find(|_| true)
    }
}

impl Visitor<Attribute> for Doc {
    type Result = Option<String>;

    fn visit(&self, item: &Attribute) -> Self::Result {
        if item.attr_name.iter().any(|name| name == "Description") {
            Some(item.attr_value.to_string())
        } else {
            None
        }
    }
}

pub struct FieldName(String);

impl<T> From<T> for FieldName
where
    T: Into<String>,
{
    fn from(name: T) -> Self {
        let name = name.into().to_snake_case();
        if RUST_KEYWORDS.contains(&name.as_str()) {
            FieldName(format!("{}_", name))
        } else {
            FieldName(name)
        }
    }
}

const RUST_KEYWORDS: [&str; 1] = ["type"];

impl AsRef<str> for FieldName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Display for FieldName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Field> for codegen::Type {
    fn from(field: Field) -> Self {
        if field.field_type.nullable().is_some() {
            codegen::Type::from(field.field_type)
        } else {
            match field.field_modifier {
                FieldModifier::Optional => {
                    let mut type_ = codegen::Type::new("Option");
                    type_.generic(codegen::Type::from(field.field_type));
                    type_
                }
                FieldModifier::Required => codegen::Type::from(field.field_type),
            }
        }
    }
}

impl From<Type> for codegen::Type {
    fn from(type_: Type) -> Self {
        match type_ {
            Type::Basic(type_) => type_.into(),
            Type::Complex(type_) => type_.into(),
        }
    }
}

impl From<BasicType> for codegen::Type {
    fn from(type_: BasicType) -> codegen::Type {
        let name = match type_ {
            BasicType::Bool => "bool",
            BasicType::UInt8 => "u8",
            BasicType::UInt16 => "u16",
            BasicType::UInt32 => "u32",
            BasicType::UInt64 => "u64",
            BasicType::Int8 => "i8",
            BasicType::Int16 => "i16",
            BasicType::Int32 => "i32",
            BasicType::Int64 => "i64",
            BasicType::Float => "f32",
            BasicType::Double => "f64",
            BasicType::String => "String",
            BasicType::WString => "String",
        };

        codegen::Type::new(name)
    }
}

impl From<ComplexType> for codegen::Type {
    fn from(type_: ComplexType) -> codegen::Type {
        match type_ {
            ComplexType::Map { key, element } => {
                let mut type_ = codegen::Type::new("std::collections::HashMap");

                let key = Type::from_str(&key).expect("unexpected type: key");
                type_.generic(key);

                let element = Type::from_str(&element).expect("unexpected type: element");
                type_.generic(element);
                type_
            }
            ComplexType::Parameter { value } => codegen::Type::new(&value.param_name),
            ComplexType::Vector { element } => {
                let type_: Type = *element;
                type_.into()
            }
            ComplexType::Nullable { element } => {
                let mut type_ = codegen::Type::new("Option");
                let element = *element;
                type_.generic(element);
                type_
            }
            ComplexType::User { declaration } => {
                let name = match *declaration {
                    UserTypeDeclaration::Struct(struct_) => struct_.decl_name.to_string(),
                    UserTypeDeclaration::Enum(enum_) => enum_.decl_name.to_string(),
                };
                codegen::Type::new(&format!("crate::contracts::{}", name))
            }
        }
    }
}