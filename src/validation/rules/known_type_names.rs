use ast::{Fragment, InlineFragment, VariableDefinition};
use validation::{ValidatorContext, Visitor};
use parser::{SourcePosition, Spanning};

pub struct KnownTypeNames {}

pub fn factory() -> KnownTypeNames {
    KnownTypeNames {}
}

impl<'a> Visitor<'a> for KnownTypeNames {
    fn enter_inline_fragment(&mut self, ctx: &mut ValidatorContext<'a>, fragment: &'a Spanning<InlineFragment>) {
        if let Some(ref type_cond) = fragment.item.type_condition {
            validate_type(ctx, &type_cond.item, &type_cond.start);
        }
    }

    fn enter_fragment_definition(&mut self, ctx: &mut ValidatorContext<'a>, fragment: &'a Spanning<Fragment>) {
        let type_cond = &fragment.item.type_condition;
        validate_type(ctx, &type_cond.item, &type_cond.start);
    }

    fn enter_variable_definition(&mut self, ctx: &mut ValidatorContext<'a>, &(_, ref var_def): &'a (Spanning<String>, VariableDefinition)) {
        let type_name = var_def.var_type.item.innermost_name();
        validate_type(ctx, &type_name, &var_def.var_type.start);
    }
}

fn validate_type<'a>(ctx: &mut ValidatorContext<'a>, type_name: &str, location: &SourcePosition) {
    if ctx.schema.type_by_name(type_name).is_none() {
        ctx.report_error(
            &error_message(type_name),
            &[location.clone()]);
    }
}

fn error_message(type_name: &str) -> String {
    format!(r#"Unknown type "{}""#, type_name)
}

#[cfg(test)]
mod tests {
    use super::{error_message, factory};

    use parser::SourcePosition;
    use validation::{RuleError, expect_passes_rule, expect_fails_rule};

    #[test]
    fn known_type_names_are_valid() {
        expect_passes_rule(factory, r#"
          query Foo($var: String, $required: [String!]!) {
            user(id: 4) {
              pets { ... on Pet { name }, ...PetFields, ... { name } }
            }
          }
          fragment PetFields on Pet {
            name
          }
        "#);
    }

    #[test]
    fn unknown_type_names_are_invalid() {
        expect_fails_rule(factory, r#"
          query Foo($var: JumbledUpLetters) {
            user(id: 4) {
              name
              pets { ... on Badger { name }, ...PetFields }
            }
          }
          fragment PetFields on Peettt {
            name
          }
        "#,
            &[
                RuleError::new(&error_message("JumbledUpLetters"), &[
                    SourcePosition::new(27, 1, 26),
                ]),
                RuleError::new(&error_message("Badger"), &[
                    SourcePosition::new(120, 4, 28),
                ]),
                RuleError::new(&error_message("Peettt"), &[
                    SourcePosition::new(210, 7, 32),
                ]),
            ]);
    }
}
