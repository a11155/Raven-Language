use syntax::code::Effects;
use syntax::ParsingError;

use crate::parser::code_parser::parse_line;
use crate::{ParserUtils, TokenTypes};

pub fn parse_operator(last: Option<Effects>, parser_utils: &mut ParserUtils) -> Result<Effects, ParsingError> {
    let mut operation = String::new();
    let mut effects = Vec::new();

    if let Some(effect) = last {
        operation += "{}";
        effects.push(effect);
    }

    parser_utils.index -= 1;
    while let Some(token) = parser_utils.tokens.get(parser_utils.index) {
        if token.token_type == TokenTypes::Operator || token.token_type == TokenTypes::Equals {
            operation += token.to_string(parser_utils.buffer).as_str();
        } else {
            break
        }
        parser_utils.index += 1;
    }

    let mut right = parse_line(parser_utils, true, false)?.map(|inner| inner.effect);
    if right.is_some() {
        while parser_utils.tokens.get(parser_utils.index-1).unwrap().token_type == TokenTypes::ArgumentEnd {
            let next = parse_line(parser_utils, true, false)?.map(|inner| inner.effect);
            if let Some(found) = next {
                right = match right.unwrap() {
                    Effects::CreateArray(mut inner) => {
                        inner.push(found);
                        Some(Effects::CreateArray(inner))
                    },
                    other => Some(Effects::CreateArray(vec!(other, found)))
                };
            } else {
                break
            }
        }

        if let Some(inner) = right {
            if let Effects::NOP() = inner {

            } else {
                operation += "{}";
            }
        }
    }

    let mut last_token;
    loop {
        last_token = parser_utils.tokens.get(parser_utils.index).unwrap();
        if last_token.token_type == TokenTypes::Operator {
            operation += last_token.to_string(parser_utils.buffer).as_str();
        } else {
            break
        }
        parser_utils.index += 1;
    }

    return Ok(Effects::Operation(operation, effects));
}