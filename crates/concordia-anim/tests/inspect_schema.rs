//! `anim.inspect`'s schemas are generated from its reflected params and result (2.4.4).

use concordia_api::{MethodInfo, params_schema, result_schema};

#[test]
fn anim_inspect_schemas_are_reflect_generated() {
    let method = MethodInfo::of::<concordia_anim::AnimInspect>();
    insta::assert_json_snapshot!("anim_inspect_params", params_schema(&method));
    insta::assert_json_snapshot!("anim_inspect_result", result_schema(&method));
}
