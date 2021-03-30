use std::{
    fs::File,
    io::{BufWriter, Cursor, Read, Seek},
    path::{Path, PathBuf},
};

use clap::Clap;
use exocore_core::{
    cell::{Application, Cell, CellApplicationConfigExt, CellConfigExt, ManifestExt},
    sec::{
        hash::{multihash_sha3_256_file, MultihashExt},
        keys::Keypair,
    },
};
use exocore_protos::{
    apps::Manifest,
    core::{cell_application_config, CellApplicationConfig, CellConfig},
};
use tempfile::{tempdir, TempDir};
use zip::write::FileOptions;

use crate::{
    term::{print_error, print_info, print_success, print_warning, style_value},
    utils::expand_tild,
    Context,
};

#[derive(Clap)]
pub struct AppOptions {
    #[clap(subcommand)]
    pub command: AppCommand,
}

#[derive(Clap)]
pub enum AppCommand {
    /// Generate an application structure in current directory.
    Generate(GenerateOptions),

    /// Package an application.
    Package(PackageOptions),
}

#[derive(Clap)]
pub struct GenerateOptions {
    /// Application name
    name: String,
}

#[derive(Clap)]
pub struct PackageOptions {
    directory: Option<PathBuf>,
}

pub async fn handle_cmd(ctx: &Context, app_opts: &AppOptions) {
    match &app_opts.command {
        AppCommand::Generate(gen_opts) => cmd_generate(ctx, app_opts, gen_opts),
        AppCommand::Package(pkg_opts) => cmd_package(ctx, app_opts, pkg_opts),
    }
}

fn cmd_generate(_ctx: &Context, _app_opts: &AppOptions, gen_opts: &GenerateOptions) {
    let cur_dir = std::env::current_dir().expect("Couldn't get current directory");

    let kp = Keypair::generate_ed25519();

    let manifest = Manifest {
        name: gen_opts.name.clone(),
        version: "0.0.1".to_string(),
        public_key: kp.public().encode_base58_string(),
        path: String::new(),
        schemas: Vec::new(),
        module: None,
    };

    let manifest_path = cur_dir.join("app.yaml");
    let manifest_file = File::create(manifest_path).expect("Couldn't create manifest file");
    manifest
        .to_yaml_writer(manifest_file)
        .expect("Couldn't write manifest");

    print_success(format!(
        "Application {} generated.",
        style_value(&gen_opts.name)
    ));
    print_warning(format!(
        "Application keypair (to be saved securely!): {}",
        style_value(kp.encode_base58_string())
    ));
}

fn cmd_package(_ctx: &Context, _app_opts: &AppOptions, pkg_opts: &PackageOptions) {
    let cur_dir = std::env::current_dir().expect("Couldn't get current directory");

    let app_dir = pkg_opts
        .directory
        .clone()
        .unwrap_or_else(|| cur_dir.clone());
    let app_dir = expand_tild(app_dir).expect("Couldn't expand app directory");

    let manifest_path = app_dir.join("app.yaml");
    let mut manifest_abs =
        Manifest::from_yaml_file(manifest_path).expect("Couldn't read manifest file");

    if let Some(module) = &mut manifest_abs.module {
        module.multihash = multihash_sha3_256_file(&module.file)
            .expect("Couldn't multihash module")
            .encode_bs58();
    }

    let mut manifest_rel = manifest_abs.clone();
    manifest_rel.make_relative_paths(&app_dir);

    let zip_file_path = cur_dir.join(format!("{}.zip", manifest_abs.name));
    let zip_file = File::create(&zip_file_path).expect("Couldn't create zip file");
    let zip_file_buf = BufWriter::new(zip_file);

    let mut zip_archive = zip::ZipWriter::new(zip_file_buf);

    zip_archive
        .start_file("app.yaml", FileOptions::default())
        .expect("Couldn't start zip file");
    manifest_rel
        .to_yaml_writer(&mut zip_archive)
        .expect("Couldn't write manifest to zip");

    if let Some(module) = &manifest_abs.module {
        zip_archive
            .start_file(&module.file, FileOptions::default())
            .expect("Couldn't start zip file");

        let mut module_file = File::open(&module.file).expect("Couldn't open app module");
        std::io::copy(&mut module_file, &mut zip_archive).expect("Couldn't copy app module to zip");
    }

    for schema in &manifest_rel.schemas {
        if let Some(exocore_protos::apps::manifest_schema::Source::File(file)) = &schema.source {
            zip_archive
                .start_file(file, FileOptions::default())
                .expect("Couldn't start zip file");

            let abs_file = app_dir.join(file);
            let mut schema_file = File::open(&abs_file).expect("Couldn't open app schema");
            std::io::copy(&mut schema_file, &mut zip_archive)
                .expect("Couldn't copy app schema to zip");
        }
    }

    zip_archive.finish().expect("Couldn't finished zip file");

    print_success(format!(
        "Application {} version {} got packaged to {}",
        style_value(manifest_abs.name),
        style_value(manifest_abs.version),
        style_value(zip_file_path),
    ));
}

pub async fn app_install(
    cell: &Cell,
    cell_config: &mut CellConfig,
    url: &str,
    overwrite: bool,
) -> anyhow::Result<AppPackage> {
    let pkg = fetch_package_url(url)
        .await
        .expect("Couldn't fetch app package");

    let apps_dir = cell.apps_directory().expect("No apps directory");
    std::fs::create_dir_all(apps_dir).expect("Couldn't create app dir");

    let app_dir = cell.app_directory(pkg.app.manifest()).unwrap();
    let cell_dir = cell.cell_directory().unwrap();

    let application = Application::new_from_directory(pkg.temp_dir.path())?;
    application
        .validate()
        .expect("Failed to validate the application");

    if app_dir.exists() {
        if overwrite {
            print_info(format!(
                "Application already installed at '{}'. Overwriting it.",
                style_value(&app_dir)
            ));
            std::fs::remove_dir_all(&app_dir).expect("Couldn't remove existing app dir");
        } else {
            print_error(format!(
                "Application already installed at '{}'. Use {} to overwrite it.",
                style_value(&app_dir),
                style_value("--force"),
            ));
            return Ok(pkg);
        }
    }

    std::fs::rename(&pkg.temp_dir, &app_dir).expect("Couldn't move temp app dir");

    let mut cell_app_config = CellApplicationConfig::from_manifest(pkg.app.manifest().clone());
    cell_app_config.location = Some(cell_application_config::Location::Path(
        app_dir
            .strip_prefix(cell_dir)
            .unwrap()
            .to_string_lossy()
            .into(),
    ));
    cell_app_config.package_url = url.to_string();

    cell_config.add_application(cell_app_config);

    Ok(pkg)
}

pub async fn fetch_package_url<U: Into<String>>(url: U) -> anyhow::Result<AppPackage> {
    let url: String = url.into();
    if let Some(path) = url.strip_prefix("file://") {
        return read_package_path(path);
    }

    let fetch_resp = reqwest::get(url)
        .await
        .map_err(|err| anyhow!("Couldn't fetch package: {}", err))?;

    let package_bytes = fetch_resp
        .bytes()
        .await
        .map_err(|err| anyhow!("Couldn't fetch bytes for package: {}", err))?;

    let cursor = Cursor::new(package_bytes.as_ref());
    read_package(cursor)
}

pub fn read_package_path<P: AsRef<Path>>(path: P) -> anyhow::Result<AppPackage> {
    let file =
        File::open(path.as_ref()).map_err(|err| anyhow!("Couldn't open package file: {}", err))?;

    read_package(file)
}

pub struct AppPackage {
    pub app: Application,
    pub temp_dir: TempDir,
}

pub fn read_package<R: Read + Seek>(reader: R) -> anyhow::Result<AppPackage> {
    let mut package_zip = zip::ZipArchive::new(reader)
        .map_err(|err| anyhow!("Couldn't read package zip: {}", err))?;

    let dir = tempdir().map_err(|err| anyhow!("Couldn't create temp dir: {}", err))?;

    package_zip
        .extract(dir.path())
        .map_err(|err| anyhow!("Couldn't extract package: {}", err))?;

    let manifest = Manifest::from_yaml_file(dir.path().join("app.yaml"))
        .map_err(|err| anyhow!("Couldn't read manifest from package: {}", err))?;

    let app = Application::new_from_manifest(manifest)
        .map_err(|err| anyhow!("Couldn't create app from manifest: {}", err))?;

    Ok(AppPackage { app, temp_dir: dir })
}
