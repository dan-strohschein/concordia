//! `concordia cluster stats`: how a model's meshes were split into meshlets, or why they weren't
//! (M4 4.2; the same data as `asset.meshlet_stats`).

use std::path::{Path, PathBuf};

use concordia_assets::{AssetDb, AssetMeshletStatsResult, AssetRef, MeshMeshlets};
use concordia_cluster::MeshletStats;
use concordia_core::id::SystemIdGen;

use crate::Failure;

/// `concordia cluster` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct ClusterArgs {
    #[command(subcommand)]
    command: ClusterCommand,
}

#[derive(Debug, clap::Subcommand)]
enum ClusterCommand {
    /// Print meshlet statistics for a model's meshes (count, triangles and vertices per meshlet,
    /// bounds, normal cones), or why a mesh has none.
    Stats(StatsArgs),
}

/// `concordia cluster stats` arguments.
#[derive(Debug, clap::Args)]
struct StatsArgs {
    /// Project root.
    #[arg(long, default_value = ".")]
    project: PathBuf,
    /// Print JSON (the `asset.meshlet_stats` result) instead of text.
    #[arg(long)]
    json: bool,
    /// A project-relative model file (imported first, so its stats are current) or an asset
    /// reference ("asset:<id>" or "asset:<id>#mesh/<name>/<part>").
    asset: String,
}

pub(crate) fn run(args: &ClusterArgs) -> Result<(), Failure> {
    match &args.command {
        ClusterCommand::Stats(stats) => print_stats(stats),
    }
}

fn print_stats(args: &StatsArgs) -> Result<(), Failure> {
    let fail = |e: concordia_assets::AssetError| Failure::usage(e.to_string());
    let mut db = AssetDb::new(&args.project, Box::new(SystemIdGen)).map_err(fail)?;
    for importer in concordia_app::asset_importers() {
        db.register(importer);
    }
    db.scan().map_err(fail)?;
    let asset = resolve(&mut db, &args.asset).map_err(fail)?;
    let meshes = db.meshlet_stats(&asset).map_err(fail)?;
    if args.json {
        let result = AssetMeshletStatsResult { meshes };
        let text =
            serde_json::to_string_pretty(&result).map_err(|e| Failure::usage(e.to_string()))?;
        println!("{text}");
    } else {
        print!("{}", text(&args.asset, &meshes));
    }
    Ok(())
}

/// An asset reference as given, or the asset a file path imports to.
fn resolve(db: &mut AssetDb, asset: &str) -> Result<AssetRef, concordia_assets::AssetError> {
    if asset.starts_with("asset:") {
        return asset.parse();
    }
    let report = db.import(Path::new(asset))?;
    Ok(AssetRef::Asset {
        id: report.id,
        sub: None,
    })
}

/// The text report.
fn text(asset: &str, meshes: &[MeshMeshlets]) -> String {
    let mut out = format!("{asset}\n");
    for mesh in meshes {
        match (&mesh.meshlets, &mesh.missing) {
            (Some(stats), _) => out.push_str(&describe(&mesh.sub_asset, stats)),
            (None, why) => out.push_str(&format!(
                "  {}: {}, no meshlets ({})\n",
                mesh.sub_asset,
                match mesh.triangles {
                    1 => "1 triangle".to_owned(),
                    n => format!("{n} triangles"),
                },
                why.as_deref().unwrap_or("not built")
            )),
        }
    }
    out
}

fn describe(sub: &str, s: &MeshletStats) -> String {
    let cones = match s.mean_cone_half_angle_deg {
        Some(angle) => format!("mean half-angle {angle:.1}°"),
        None => "none can cull".into(),
    };
    format!(
        "  {sub}: {} triangles, {} vertices -> {} meshlets ({} triangles, {} vertices, cone weight {})\n\
         \x20   triangles per meshlet  min {:.0}  mean {:.1}  max {:.0}  (fill {:.1}%)\n\
         \x20   vertices per meshlet   min {:.0}  mean {:.1}  max {:.0}\n\
         \x20   sphere radius          min {:.4}  mean {:.4}  max {:.4}\n\
         \x20   normal cones           {:.1}% can cull, {cones}\n\
         \x20   encoded                {} bytes\n",
        s.source_triangles,
        s.source_vertices,
        s.meshlets,
        s.max_triangles,
        s.max_vertices,
        s.cone_weight,
        s.triangles_per_meshlet.min,
        s.triangles_per_meshlet.mean,
        s.triangles_per_meshlet.max,
        100.0 * s.triangle_fill,
        s.vertices_per_meshlet.min,
        s.vertices_per_meshlet.mean,
        s.vertices_per_meshlet.max,
        s.radius.min,
        s.radius.mean,
        s.radius.max,
        100.0 * s.cullable_share,
        s.encoded_bytes,
    )
}
