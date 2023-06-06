use std::sync::Arc;
use indexmap::IndexMap;
use async_recursion::async_recursion;
use syntax::{Attribute, get_modifier, is_modifier, Modifier, ParsingError, ParsingFuture, TraitImplementor};
use syntax::async_util::{NameResolver, UnparsedType};
use syntax::code::{Field, MemberField};
use syntax::r#struct::Struct;
use syntax::syntax::{ParsingType, Syntax};
use syntax::types::Types;
use crate::parser;
use crate::parser::function_parser::parse_function;
use crate::parser::top_parser::{parse_attribute, parse_import, parse_modifier};
use crate::parser::util::{add_generics, ParserUtils};
use crate::tokens::tokens::{Token, TokenTypes};

pub fn parse_structure(parser_utils: &mut ParserUtils, attributes: Vec<Attribute>, modifiers: Vec<Modifier>)
                       -> Result<Struct, ParsingError> {
    let modifiers = get_modifier(modifiers.as_slice());

    let mut member_modifiers = Vec::new();
    let mut member_attributes = Vec::new();

    let mut name = String::new();
    let mut fields = Vec::new();
    let mut generics = IndexMap::new();
    while parser_utils.tokens.len() != parser_utils.index {
        let token: Token = parser_utils.tokens.get(parser_utils.index).unwrap().clone();
        parser_utils.index += 1;
        match token.token_type {
            TokenTypes::Identifier => {
                name = token.to_string(parser_utils.buffer);
                if !is_modifier(modifiers, Modifier::Internal) || is_modifier(modifiers, Modifier::Trait) {
                    name = parser_utils.file.clone() + "::" + name.as_str();
                }

                parser_utils.imports.parent = Some(name.clone());
            }
            TokenTypes::GenericsStart => parse_generics(parser_utils, &mut generics),
            TokenTypes::StructTopElement | TokenTypes::Comment => {}
            TokenTypes::InvalidCharacters => parser_utils.syntax.lock().unwrap()
                .add_poison(Arc::new(Struct::new_poisoned(format!("{}", parser_utils.file),
                                                          token.make_error(parser_utils.file.clone(),
                                                                           "Unexpected top element!".to_string())))),
            TokenTypes::ImportStart => parse_import(parser_utils),
            TokenTypes::AttributesStart => parse_attribute(parser_utils, &mut member_attributes),
            TokenTypes::ModifiersStart => parse_modifier(parser_utils, &mut member_modifiers),
            TokenTypes::FunctionStart => {
                let file = parser_utils.file.clone();
                parser_utils.file = name.clone();
                let function = parse_function(parser_utils, member_attributes, member_modifiers);
                ParserUtils::add_function(parser_utils.syntax.clone(), &parser_utils.handle, Box::new(parser_utils.imports.clone()),
                                              parser_utils.file.clone(), token.clone(), function);
                parser_utils.file = file;
                member_attributes = Vec::new();
                member_modifiers = Vec::new();
            }
            TokenTypes::FieldName => {
                fields.push(parse_field(parser_utils, token.to_string(parser_utils.buffer),
                                        member_attributes, member_modifiers));
                member_attributes = Vec::new();
                member_modifiers = Vec::new();
            }
            TokenTypes::StructEnd => break,
            TokenTypes::EOF => break,
            _ => panic!("How'd you get here? {:?}", token.token_type)
        }
    }

    return Ok(Struct::new(attributes, fields, generics, modifiers, name));
}

pub fn parse_implementor(parser_utils: &mut ParserUtils, attributes: Vec<Attribute>,
                         _modifiers: Vec<Modifier>) -> Result<TraitImplementor, ParsingError> {
    let mut base = None;
    let mut implementor = None;
    let mut member_attributes = Vec::new();
    let mut member_modifiers = Vec::new();
    let mut functions = Vec::new();
    let mut generics = IndexMap::new();

    let mut state = 0;
    while parser_utils.tokens.len() != parser_utils.index {
        let token = parser_utils.tokens.get(parser_utils.index).unwrap();
        parser_utils.index += 1;
        match token.token_type {
            TokenTypes::Identifier => {
                let name = token.to_string(parser_utils.buffer);
                let temp = Some(UnparsedType::Basic(name.clone()));
                if state == 0 {
                    base = temp;
                    state = 1;
                } else {
                    parser_utils.imports.parent = Some(format!("{}::{}", parser_utils.file.clone(), name));
                    implementor = temp;
                }
            }
            TokenTypes::GenericsStart => {
                if state == 0 {
                    parse_generics(parser_utils, &mut generics);
                } else {
                    let found =
                        Box::pin(parse_type_generics(parser_utils));
                    if state == 1 {
                        base = Some(found);
                    } else {
                        implementor = Some(found);
                    }
                }
            }
            TokenTypes::For => state = 2,
            TokenTypes::AttributesStart => parse_attribute(parser_utils, &mut member_attributes),
            TokenTypes::ModifiersStart => parse_modifier(parser_utils, &mut member_modifiers),
            TokenTypes::FunctionStart => {
                functions.push(parse_function(parser_utils, member_attributes, member_modifiers)?);
                member_attributes = Vec::new();
                member_modifiers = Vec::new();
            }
            TokenTypes::StructTopElement => {}
            TokenTypes::StructEnd | TokenTypes::EOF => break,
            _ => panic!("How'd you get here? {:?}", token.token_type)
        }
    }

    let token = parser_utils.tokens.get(parser_utils.index - 1).unwrap();

    let base = Box::pin(
        Syntax::parse_type(
            parser_utils.syntax.clone(),
            token.make_error(parser_utils.file.clone(), format!("Failed to find")),
            parser_utils.imports.boxed_clone(), base.unwrap()));

    let implementor = Box::pin(
        Syntax::parse_type(
            parser_utils.syntax.clone(),
            token.make_error(parser_utils.file.clone(), format!("Failed to find")),
            parser_utils.imports.boxed_clone(), implementor.unwrap()));

    return Ok(TraitImplementor {
        base: ParsingType::Parsing(base),
        implementor: ParsingType::Parsing(implementor),
        functions,
        attributes,
    });
}

#[async_recursion]
pub async fn parse_type_generics(parser_utils: &mut ParserUtils) -> Result<Vec<Types>, ParsingError> {
    let mut current = Vec::new();
    while parser_utils.tokens.len() != parser_utils.index {
        let token = parser_utils.tokens.get(parser_utils.index).unwrap();
        parser_utils.index += 1;
        match token.token_type {
            TokenTypes::GenericsStart => {
                let mut temp = current.pop().unwrap();
                current.push(Types::GenericType(Box::new(temp),
                                                parse_type_generics(parser_utils).await?));
            }
            TokenTypes::Generic => {
                let name = token.to_string(parser_utils.buffer);
                current.push(parser_utils.get_struct(token, name).await?);
            }
            TokenTypes::GenericEnd => {
                break;
            }
            _ => {
                panic!("Unexpected type!");
            }
        }
    }

    return Ok(current);
}

pub fn parse_generics(parser_utils: &mut ParserUtils, generics: &mut IndexMap<String, ParsingType<Types>>) {
    let mut name = String::new();
    let mut unfinished_bounds = Vec::new();
    let mut bounds: Option<UnparsedType> = None;
    while parser_utils.tokens.len() != parser_utils.index {
        let token = parser_utils.tokens.get(parser_utils.index).unwrap();
        parser_utils.index += 1;
        match token.token_type {
            TokenTypes::Generic => {
                name = token.to_string(parser_utils.buffer);
            }
            TokenTypes::GenericEnd => {
                parser_utils.imports.generics.insert(name.clone(), Syntax::parse_type(parser_utils.syntax.clone(),
                                                                                      token.make_error(parser_utils.file.clone(),
                                                                                                       format!("")),
                                                                                      Box::new(parser_utils.imports.clone()), bounds.unwrap()));
                generics.insert(name.clone(), Syntax::parse_type(parser_utils.syntax.clone(),
                                                   token.make_error(parser_utils.file.clone(),
                                                                    format!("")),
                                                   Box::new(parser_utils.imports.clone()), bounds.unwrap()));
                bounds = None;
                unfinished_bounds = Vec::new();
            }
            TokenTypes::GenericBound => {
                let token = parser_utils.tokens.get(parser_utils.index).unwrap();
                parser_utils.index += 1;
                let name = token.to_string(parser_utils.buffer);
                bounds = Some(add_generics(UnparsedType::Basic(name), parser_utils));
            }
            _ => {
                parser_utils.index -= 1;
                return;
            }
        }
    }
}

pub fn parse_field(parser_utils: &mut ParserUtils, name: String,
                   attributes: Vec<Attribute>, modifiers: Vec<Modifier>) -> ParsingType<MemberField> {
    let mut types = None;
    while !parser_utils.tokens.is_empty() {
        let token = parser_utils.tokens.get(parser_utils.index).unwrap();
        parser_utils.index += 1;
        match token.token_type {
            TokenTypes::FieldType => {
                let name = token.to_string(parser_utils.buffer).clone();
                types = Some(parser_utils.get_struct(token, name))
            }
            TokenTypes::FieldSeparator => {}
            TokenTypes::FieldEnd => break,
            _ => panic!("How'd you get here? {:?}", token.token_type)
        }
    }

    return ParsingType::Parsing(Box::pin(to_field(types.unwrap(), attributes, get_modifier(modifiers.as_slice()), name)));
}

pub async fn to_field(types: ParsingFuture<Types>, attributes: Vec<Attribute>, modifier: u8, name: String) -> Result<MemberField, ParsingError> {
    return Ok(MemberField::new(modifier, attributes, Field::new(name, types.await?)));
}