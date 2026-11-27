// Anti-Horizon Launcher: a fast Minecraft: Java Edition launcher.
// The Rust side does everything heavy (version lists, parallel verified downloads, Java, launching, Microsoft
// sign-in, Modrinth and CurseForge); the window is a small web page in the Anti-Horizon gold-and-space look.
// A window app in every build: no console window opens beside it, not even when running the development copy.
#![windows_subsystem = "windows"]

mod auth;
mod console;
mod content;
mod deps;
mod installed;
mod mc;
mod mods;
mod net;
mod presets;
mod skins;
mod store;
mod tweaks;
mod update;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            store::get_settings,
            store::save_settings,
            store::open_data_folder,
            store::open_url,
            store::system_memory_mb,
            mc::get_versions,
            mc::get_loader_versions,
            mc::list_instances,
            mc::create_instance,
            mc::delete_instance,
            mc::install_instance,
            mc::launch_instance,
            mc::stop_instance,
            mc::open_instance_folder,
            console::get_game_log,
            update::update_check,
            update::update_install,
            mods::search_mods,
            mods::project_details,
            mods::features,
            mods::install_mod,
            mods::install_modpack,
            mods::list_mods,
            mods::remove_mod,
            mods::toggle_mod,
            content::list_datapacks,
            deps::fix_dependencies,
            content::toggle_datapack,
            content::remove_datapack,
            content::list_dir,
            content::open_dir,
            content::list_worlds,
            content::open_world,
            content::delete_world,
            content::export_mrpack,
            content::import_mrpack,
            skins::get_profile,
            skins::upload_skin,
            skins::reset_skin,
            skins::set_cape,
            skins::default_skins,
            skins::skin_packs,
            skins::add_to_library,
            skins::import_skin_pack,
            skins::delete_library_skin,
            skins::delete_skin_pack,
            installed::check_updates,
            installed::update_mods,
            auth::login_start,
            auth::login_poll,
            auth::login_cancel,
            auth::list_accounts,
            auth::select_account,
            auth::remove_account,
        ])
        .run(tauri::generate_context!())
        .expect("the launcher could not start");
}
