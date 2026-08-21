//! Capa de escritorio: expone el núcleo como comandos de Tauri.

// Públicos para que `tests/preview.rs` pueda llamar a los comandos de vista previa —los que no
// tocan la red— con la carga que manda la interfaz. Es la única forma de probar la traducción de
// argumentos sin levantar la ventana.
pub mod commands;
pub mod process;
pub mod state;

use tauri::Manager;
use tauri_plugin_log::{Target, TargetKind};

/// Cuánto puede crecer el archivo de registro antes de rotar, y cuántos se conservan.
///
/// Un archivo suelto que crece sin techo termina siendo inútil de dos maneras: ocupa lugar y nadie
/// lo abre. Con dos megas alcanza para varias sesiones de trabajo, y guardar los anteriores permite
/// mirar la corrida donde pasó el problema y no solo la actual.
const LOG_MAX_BYTES: u128 = 2 * 1024 * 1024;
const LOG_KEEP: usize = 3;

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                // Al archivo va todo lo que importa; a la consola, para `pnpm dev`. La interfaz
                // escribe en el mismo archivo desde `ipc.ts`, que es por donde pasa todo error que
                // el usuario llega a ver.
                .targets([
                    Target::new(TargetKind::LogDir {
                        file_name: Some("pgforge".to_owned()),
                    }),
                    Target::new(TargetKind::Stdout),
                ])
                .max_file_size(LOG_MAX_BYTES)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(LOG_KEEP))
                .level(tauri_plugin_log::log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        // Para abrir la página de una release en el navegador del sistema. Un `<a href>` adentro de
        // la ventana navegaría la propia aplicación a GitHub, que es lo último que se quiere.
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            app.manage(state::AppState::new(config_dir)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::servers::list_profiles,
            commands::servers::list_groups,
            commands::servers::rename_group,
            commands::servers::save_profile,
            commands::servers::import_scan,
            commands::servers::import_apply,
            commands::servers::delete_profile,
            commands::servers::connect,
            commands::servers::ssh_test,
            commands::servers::disconnect,
            commands::servers::connected_servers,
            commands::schema::tree_children,
            commands::schema::tree_search,
            commands::schema::read_cancel,
            commands::schema::object_ddl,
            commands::schema::schema_graph,
            commands::compare::schema_compare,
            commands::compare::schema_names,
            commands::schema::erd_export_svg,
            commands::monitoring::monitor_start,
            commands::monitoring::monitor_stop,
            commands::monitoring::monitor_configure,
            commands::monitoring::monitor_refresh,
            commands::monitoring::backend_locks,
            commands::monitoring::cancel_backend,
            commands::monitoring::terminate_backend,
            commands::monitoring::table_stats,
            commands::monitoring::index_stats,
            commands::monitoring::redundant_indexes,
            commands::monitoring::has_statement_stats,
            commands::monitoring::statement_stats,
            commands::monitoring::has_bloat_stats,
            commands::monitoring::table_bloat,
            commands::monitoring::maintenance_plan,
            commands::monitoring::maintenance_run,
            commands::tasks::process_watch,
            commands::tasks::process_cancel,
            commands::tasks::process_remove,
            commands::tasks::process_clear,
            commands::query::query_open,
            commands::query::query_close,
            commands::query::query_run,
            commands::query::query_cancel,
            commands::query::query_commit,
            commands::query::query_rollback,
            commands::query::query_autocommit,
            commands::query::query_tx_status,
            commands::query::query_column_types,
            commands::query::query_explain,
            commands::query::statement_at_cursor,
            commands::query::sql_write_file,
            commands::query::sql_read_file,
            commands::query::schema_snapshot,
            commands::query::history_recent,
            commands::query::history_search,
            commands::query::history_clear,
            commands::query::saved_list,
            commands::query::saved_save,
            commands::query::saved_delete,
            commands::query::snippets_list,
            commands::query::snippet_save,
            commands::query::snippet_delete,
            commands::query::snippets_reset,
            commands::data::data_open,
            commands::data::data_shape_named,
            commands::data::data_page,
            commands::data::data_preview,
            commands::data::data_apply,
            commands::data::data_export_preview,
            commands::data::data_export_run,
            commands::data::data_import_preview,
            commands::data::data_import_run,
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
            commands::ddl::role_names,
            commands::ddl::privilege_preview,
            commands::ddl::privilege_apply,
            commands::ddl::relation_privileges,
            commands::ddl::schema_privileges,
            commands::ddl::function_privileges,
            commands::ddl::database_privileges,
            commands::ddl::column_privileges,
            commands::ddl::default_privileges,
            commands::ddl::policy_preview,
            commands::ddl::policy_apply,
            commands::ddl::table_security,
            commands::ddl::sequence_preview,
            commands::ddl::sequence_apply,
            commands::ddl::sequence_info,
            commands::ddl::type_preview,
            commands::ddl::type_apply,
            commands::ddl::type_info,
            commands::ddl::domain_preview,
            commands::ddl::domain_apply,
            commands::ddl::domain_info,
            commands::ddl::schema_preview,
            commands::ddl::schema_apply,
            commands::ddl::database_preview,
            commands::ddl::database_apply,
            commands::ddl::database_info,
            commands::ddl::partition_preview,
            commands::ddl::partition_apply,
            commands::ddl::table_partitions,
            commands::ddl::comment_preview,
            commands::ddl::comment_apply,
            commands::ddl::extension_preview,
            commands::ddl::extension_apply,
            commands::ddl::extension_info,
            commands::ddl::available_extensions,
            commands::ddl::fdw_preview,
            commands::ddl::fdw_apply,
            commands::ddl::fdw_info,
            commands::ddl::available_fdws,
            commands::ddl::foreign_server_preview,
            commands::ddl::foreign_server_apply,
            commands::ddl::foreign_server_info,
            commands::ddl::user_mapping_preview,
            commands::ddl::user_mapping_apply,
            commands::ddl::user_mappings,
            commands::settings::server_settings,
            commands::settings::settings_preview,
            commands::settings::settings_apply,
            commands::query::explain_warning,
            commands::backup::backup_plan,
            commands::backup::backup_run,
            commands::backup::restore_plan,
            commands::backup::restore_run,
            commands::update::update_check,
            commands::update::update_open,
        ])
        .run(tauri::generate_context!())
        .expect("no se pudo iniciar la aplicación");
}
