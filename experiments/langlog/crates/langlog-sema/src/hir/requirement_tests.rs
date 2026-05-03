use super::*;

//= HIR.md#llg-hir-05-successful-hir-well-formedness
//= type=test
//# HIR lowering MUST reject compound semantic types that contain unknown components before constructing any HIR type.
#[test]
fn requirement_llg_hir_05_rejects_compound_semantic_types_with_unknown_components() {
    let cases = [
        SemanticType::Tuple(vec![SemanticType::U32, SemanticType::Unknown]),
        SemanticType::Array {
            element: Box::new(SemanticType::Unknown),
            length: 2,
        },
        SemanticType::Option(Box::new(SemanticType::Unknown)),
        SemanticType::Result {
            ok: Box::new(SemanticType::U32),
            err: Box::new(SemanticType::Unknown),
        },
        SemanticType::Set {
            element: Box::new(SemanticType::Unknown),
            capacity: 16,
        },
        SemanticType::Map {
            key: Box::new(SemanticType::U32),
            value: Box::new(SemanticType::Unknown),
            capacity: 16,
        },
        SemanticType::Range(Box::new(SemanticType::Unknown)),
        SemanticType::Function(FunctionType {
            params: vec![SemanticType::Unknown],
            return_type: Box::new(SemanticType::U32),
        }),
    ];

    for ty in cases {
        assert_eq!(lower_semantic_type(&ty), None, "{ty:?}");
    }
}
