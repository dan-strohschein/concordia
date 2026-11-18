//! KHR_lights_punctual lights (`light/<name>`) and cameras (`camera/<name>`).

use concordia_assets::{
    Artifact, AssetError, AssetKind, CameraData, ImportOutput, LightData, encode,
};

use crate::names::Names;

pub(crate) fn lights_and_cameras(
    doc: &gltf::Document,
    names: &Names,
    out: &mut ImportOutput,
) -> Result<(), AssetError> {
    for light in doc.lights().into_iter().flatten() {
        let (color, intensity, range) = (light.color(), light.intensity(), light.range());
        let data = match light.kind() {
            gltf::khr_lights_punctual::Kind::Directional => {
                LightData::Directional { color, intensity }
            }
            gltf::khr_lights_punctual::Kind::Point => LightData::Point {
                color,
                intensity,
                range,
            },
            gltf::khr_lights_punctual::Kind::Spot {
                inner_cone_angle,
                outer_cone_angle,
            } => LightData::Spot {
                color,
                intensity,
                range,
                inner: inner_cone_angle,
                outer: outer_cone_angle,
            },
        };
        let sub = format!("light/{}", names.lights[light.index()]);
        out.artifacts.insert(
            sub,
            Artifact {
                kind: AssetKind::Light,
                bytes: encode(&data)?,
            },
        );
    }
    for camera in doc.cameras() {
        let data = match camera.projection() {
            gltf::camera::Projection::Perspective(p) => CameraData::Perspective {
                yfov: p.yfov(),
                znear: p.znear(),
                zfar: p.zfar(),
            },
            gltf::camera::Projection::Orthographic(o) => CameraData::Orthographic {
                xmag: o.xmag(),
                ymag: o.ymag(),
                znear: o.znear(),
                zfar: o.zfar(),
            },
        };
        let sub = format!("camera/{}", names.cameras[camera.index()]);
        out.artifacts.insert(
            sub,
            Artifact {
                kind: AssetKind::Camera,
                bytes: encode(&data)?,
            },
        );
    }
    Ok(())
}
