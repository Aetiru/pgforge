//! Capa de escritorio: expone el núcleo como comandos de Tauri.

mod commands;
mod state;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            app.manage(state::AppState::new(config_dir)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::servers::list_profiles,
            commands::servers::save_profile,
            commands::servers::delete_profile,
            commands::servers::connect,
            commands::servers::disconnect,
            commands::servers::connected_servers,
            commands::schema::tree_children,
            commands::schema::object_ddl,
            commands::monitoring::monitor_start,
            commands::monitoring::monitor_stop,
            commands::monitoring::monitor_configure,
            commands::monitoring::monitor_refresh,
            commands::monitoring::backend_locks,
            commands::monitoring::cancel_backend,
            commands::monitoring::terminate_backend,
            commands::monitoring::table_stats,
            commands::monitoring::index_stats,
            commands::monitoring::has_statement_stats,
            commands::monitoring::statement_stats,
            commands::monitoring::maintenance_plan,
            commands::monitoring::maintenance_run,
            commands::monitoring::maintenance_cancel,
            commands::query::query_open,
            commands::query::query_close,
            commands::query::query_run,
            commands::query::query_cancel,
            commands::query::query_explain,
            commands::query::statement_at_cursor,
            commands::query::schema_snapshot,
            commands::query::history_recent,
            commands::query::history_search,
            commands::query::history_clear,
            commands::data::data_open,
            commands::data::data_page,
            commands::data::data_preview,
            commands::data::data_apply,
            commands::ddl::ddl_preview,
            commands::ddl::ddl_apply,
            commands::ddl::table_constraints,
            commands::ddl::index_preview,
            commands::ddl::index_create,
            commands::ddl::index_drop,
            commands::ddl::table_indexes,
            commands::ddl::view_preview,
            commands::ddl::view_apply,
            commands::ddl::view_query,
            commands::ddl::function_apply,
            commands::ddl::function_drop,
            commands::ddl::function_args,
            commands::ddl::trigger_preview,
            commands::ddl::trigger_apply,
            commands::ddl::table_triggers,
            commands::ddl::role_preview,
            commands::ddl::role_apply,
            commands::ddl::role_info,
            commands::ddl::role_memberships,
            commands::ddl::privilege_preview,
            commands::ddl::privilege_apply,
            commands::ddl::table_privileges,
            commands::ddl::schema_privileges,
            commands::query::explain_warning,
        ])
        .run(tauri::generate_context!())
        .expect("no se pudo iniciar la aplicación");
}
