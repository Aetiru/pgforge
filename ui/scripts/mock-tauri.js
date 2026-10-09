// Mock mínimo de la capa IPC de Tauri para correr la interfaz en un navegador, sin servidor ni
// ventana. Todos los datos son inventados: sirven para que cada pantalla tenga qué mostrar, no para
// probar que el núcleo diga la verdad (eso es de `crates/pgforge-core/tests`). Un comando que no está
// acá contesta `null`, así que una pantalla nueva puede fallar en la captura hasta que se le agregue su
// respuesta al objeto `handlers`.
// Mock mínimo de la capa IPC de Tauri para sacar capturas sin servidor ni ventana.
(() => {
  const PROD = { id: "p1", name: "Producción ventas", group: "Clientes", host: "db-prod.interno", port: 5432, database: "ventas", user: "analista", sslMode: "require", connectTimeoutSecs: 10, savePassword: false, environment: "prod", readOnly: false, autocommit: true };
  const DEV = { id: "p2", name: "Desarrollo", group: "Clientes", host: "localhost", port: 5417, database: "ventas", user: "postgres", sslMode: "prefer", connectTimeoutSecs: 10, savePassword: false, environment: "dev", readOnly: false, autocommit: true };
  const caps = { version: 170011, currentUser: "analista", currentDatabase: "ventas", isSuperuser: true, canSignalBackends: true, canReadAllStats: true };
  const node = (id, label, kind, extra = {}) => ({ id, label, kind, hasChildren: false, database: "ventas", ...extra });
  const folder = (id, label, f, extra = {}) => node(id, label, { folder: f }, { hasChildren: true, ...extra });
  const tree = {
    root: [folder("roles", "Roles", "roles", { database: "" }), node("db:ventas", "ventas", "database", { hasChildren: true })],
    "db:ventas": [folder("schemas", "Esquemas", "schemas")],
    schemas: [node("s:public", "public", "schema", { hasChildren: true, oid: 2200, schema: "public" })],
    "s:public": [folder("tables", "Tablas", "tables", { schema: "public" }), folder("views", "Vistas", "views", { schema: "public" })],
    tables: [node("t:clientes", "clientes", "table", { oid: 16401, schema: "public", detail: "120 k filas" }), node("t:pedidos", "pedidos", "table", { oid: 16410, schema: "public", detail: "48 k filas" }), node("t:facturas", "facturas", "table", { oid: 16422, schema: "public", detail: "9 k filas" })],
    views: [node("v:resumen", "resumen_mensual", "view", { oid: 16440, schema: "public" })],
  };
  const keyOf = (p) => (p ? (typeof p.id === "string" ? p.id : "root") : "root");
  const handlers = {
    app_info: () => ({ version: "0.9.4", minPostgresMajor: 13, logDir: null }),
    list_profiles: () => [PROD, DEV],
    connected_servers: () => ["p1", "p2"],
    list_groups: () => ["Clientes"],
    connect: ({ profile }) => ({ profile: profile ?? PROD, caps }),
    tree_children: ({ parent }) => tree[keyOf(parent)] ?? [],
    relation_dependencies: () => [
      { direction: "uses", kind: "tabla", schema: "public", name: "clientes", detail: "pedidos_cliente_fk: FOREIGN KEY (cliente_id) REFERENCES clientes(id)" },
      { direction: "usedBy", kind: "vista", schema: "public", name: "resumen_mensual", detail: null },
      { direction: "usedBy", kind: "clave foránea", schema: "public", name: "facturas", detail: "facturas_pedido_fk: FOREIGN KEY (pedido_id) REFERENCES pedidos(id)" },
    ],
    table_stat: () => ({ schema: "public", table: "pedidos", liveTuples: 48213, deadTuples: 21400, deadRatio: 0.307, totalBytes: 73400320, tableBytes: 52428800, indexBytes: 20971520, sequentialScans: 4120, indexScans: 96, lastVacuumSeconds: null, lastAutovacuumSeconds: 1900000, lastAnalyzeSeconds: null }),
    schema_privileges: () => [], database_privileges: () => [], default_privileges: () => [],
    relation_privileges: () => [], column_privileges: () => [], function_privileges: () => [],
    object_ddl: () => ({ sql: "CREATE TABLE public.pedidos (\n  id bigint PRIMARY KEY\n);", source: "pgDump" }),
    table_indexes: () => [], table_constraints: () => [], table_triggers: () => [],
    table_security: () => ({ enabled: false, forced: false, policies: [] }),
    data_open: () => ({ oid: 16410, schema: "public", name: "pedidos", columns: [], primaryKey: [], editable: false }),
    statement_at_cursor: ({ sql }) => ({ text: sql, offset: 0, line: 1 }),
    query_open: () => ({ tabId: "q1", database: "ventas", autocommit: true, txStatus: "idle" }),
    script_new_name: () => "Consulta 1",
    schema_snapshot: () => ({ database: "ventas", schemas: ["public"], relations: [
      { oid: 16410, schema: "public", name: "pedidos", columns: [{ name: "id", typeName: "bigint" }, { name: "cliente", typeName: "text" }] }] }),
    query_run: ({ channel }) => {
      const send = (message, index) => window["_cb" + channel.id]({ index, message });
      const rows = [["1001", "Ana Ferreira", "2026-09-30", "1280.50", "entregado"], ["1002", "Comercial Norte", "2026-10-01", null, "pendiente"], ["1003", "Luis Ortega", "2026-10-02", "310.00", null], ["1004", "Textiles del Sur", "2026-10-02", "5400.00", "entregado"], ["1005", null, "2026-10-03", "89.90", "cancelado"], ["1006", "Marta Quiroga", null, "742.10", "pendiente"]];
      send({ type: "started", index: 0, total: 1, line: 1, offset: 0 }, 0);
      send({ type: "finished", index: 0, outcome: { kind: "rows", columns: ["id", "cliente", "fecha", "total", "estado"], rows, rowCount: rows.length, truncated: false, seconds: 0.042 } }, 1);
      send({ type: "completed", seconds: 0.042, executed: 1 }, 2);
    },
    query_column_types: () => [{ name: "id", typeName: "bigint" }, { name: "cliente", typeName: "text" }, { name: "fecha", typeName: "date" }, { name: "total", typeName: "numeric(12,2)" }, { name: "estado", typeName: "text" }],
    query_explain: () => {
      const n = (nodeType, over = {}) => ({ nodeType, relation: null, schema: null, index: null, condition: null, filter: null, startupCost: 0, totalCost: 0, planRows: 0, actualRows: null, loops: 1, totalMs: null, selfMs: null, rowsRemoved: null, misestimated: false, sharedHitBlocks: null, sharedReadBlocks: null, sortMethod: null, sortSpaceKb: null, sortOnDisk: false, children: [], ...over });
      const root = n("Limit", { totalCost: 4210, planRows: 50, actualRows: 50, totalMs: 612, selfMs: 0.1, children: [
        n("Sort", { totalCost: 4210, planRows: 48000, actualRows: 48213, totalMs: 612, selfMs: 188, sortMethod: "external merge", sortOnDisk: true, children: [
          n("Hash Join", { totalCost: 3100, planRows: 48000, actualRows: 48213, totalMs: 420, selfMs: 64, condition: "(p.cliente_id = c.id)", children: [
            n("Seq Scan", { relation: "pedidos", schema: "public", totalCost: 1850, planRows: 48000, actualRows: 48213, totalMs: 301, selfMs: 301, rowsRemoved: 910002, sharedReadBlocks: 8120, filter: "(estado = 'pendiente')" }),
            n("Hash", { totalCost: 240, planRows: 3000, actualRows: 120000, totalMs: 55, selfMs: 55, misestimated: true, children: [
              n("Index Scan", { relation: "clientes", schema: "public", index: "clientes_pkey", totalCost: 200, planRows: 3000, actualRows: 120000, totalMs: 40, selfMs: 40 })] })] })] })] });
      return { root, planningMs: 0.9, executionMs: 612.4, analyzed: true, json: "[]", advice: [{ kind: "missingIndex", severity: "warn", title: "Seq Scan sobre pedidos descartó casi todo lo que leyó", detail: "Leyó 958 215 filas y descartó 910 002 con el filtro estado = 'pendiente'.", sql: "CREATE INDEX ON public.pedidos (estado);", index: { schema: "public", table: "pedidos", columns: ["estado"] } }] };
    },
    monitor_start: ({ channel }) => {
      const send = (message, index) => window["_cb" + channel.id]({ index, message });
      const be = (pid, o = {}) => ({ pid, database: "ventas", user: "app", applicationName: "api", clientAddr: "10.0.0.5", backendType: "client backend", state: "active", waitEventType: null, waitEvent: null, query: "SELECT 1", queryId: null, leaderPid: null, querySeconds: 0.2, transactionSeconds: 0.2, stateSeconds: 0.2, blockedBy: [], isMonitor: false, ...o });
      const backends = [
        be(4121, { state: "idle in transaction", query: "UPDATE pedidos SET estado = 'entregado' WHERE id = 1002", querySeconds: 420, transactionSeconds: 421, stateSeconds: 420 }),
        be(4188, { waitEventType: "Lock", waitEvent: "transactionid", query: "UPDATE pedidos SET total = total * 1.1 WHERE cliente_id = 7", querySeconds: 96, blockedBy: [4121] }),
        be(4190, { waitEventType: "Lock", waitEvent: "transactionid", query: "DELETE FROM pedidos WHERE id = 1002", querySeconds: 41, blockedBy: [4188] }),
        be(4203, { query: "SELECT c.nombre, sum(p.total) FROM pedidos p JOIN clientes c ON c.id = p.cliente_id GROUP BY 1", querySeconds: 8.4 }),
        be(4210, { state: "idle", query: "COMMIT" }),
      ];
      const blocking = [{ pid: 4121, blocking: [{ pid: 4188, blocking: [{ pid: 4190, blocking: [] }] }] }];
      for (let i = 0; i < 24; i++) {
        const total = 52 + Math.round(26 * Math.min(1, i / 18) + 3 * Math.sin(i));
        send({ type: "snapshot", snapshot: { backends, blocking, metrics: { totalConnections: total, activeConnections: 6 + Math.round(4 * Math.abs(Math.sin(i / 3))), idleInTransaction: 1, waitingConnections: 2, maxConnections: 100, transactionsPerSecond: 120 + 60 * Math.sin(i / 2) + i * 4, cacheHitRatio: 0.972, longestTransactionSeconds: 421 } } }, i);
      }
    },
    backend_locks: () => [{ lockType: "transactionid", relation: null, mode: "ExclusiveLock", granted: true }, { lockType: "relation", relation: "pedidos", mode: "RowExclusiveLock", granted: true }],
    monitor_stop: () => null, monitor_configure: () => null,
    schema_graph: () => {
      const col = (position, name, typeName, o = {}) => ({ position, name, typeName, notNull: false, primaryKey: false, foreignKey: false, ...o });
      const t = (oid, name, cols) => ({ oid, name, kind: "table", columns: cols });
      const tables = [
        t(1, "clientes", [col(1, "id", "bigint", { primaryKey: true, notNull: true }), col(2, "nombre", "text"), col(3, "ciudad", "text")]),
        t(2, "pedidos", [col(1, "id", "bigint", { primaryKey: true }), col(2, "cliente_id", "bigint", { foreignKey: true }), col(3, "estado", "text"), col(4, "total", "numeric(12,2)")]),
        t(3, "items", [col(1, "id", "bigint", { primaryKey: true }), col(2, "pedido_id", "bigint", { foreignKey: true }), col(3, "producto_id", "bigint", { foreignKey: true }), col(4, "cantidad", "int")]),
        t(4, "productos", [col(1, "id", "bigint", { primaryKey: true }), col(2, "nombre", "text"), col(3, "categoria_id", "bigint", { foreignKey: true }), col(4, "precio", "numeric(12,2)")]),
        t(5, "categorias", [col(1, "id", "bigint", { primaryKey: true }), col(2, "nombre", "text")]),
        t(6, "facturas", [col(1, "id", "bigint", { primaryKey: true }), col(2, "pedido_id", "bigint", { foreignKey: true }), col(3, "emitida", "date")]),
        t(7, "pagos", [col(1, "id", "bigint", { primaryKey: true }), col(2, "factura_id", "bigint", { foreignKey: true }), col(3, "monto", "numeric(12,2)")]),
        t(8, "sucursales", [col(1, "id", "int", { primaryKey: true }), col(2, "nombre", "text")]),
      ];
      const e = (name, source, target) => ({ name, source, target, sourceColumns: ["x"], targetColumns: ["id"], onUpdate: "noAction", onDelete: "noAction" });
      return { database: "ventas", schema: "public", tables, edges: [e("pedidos_cliente_fk", 2, 1), e("items_pedido_fk", 3, 2), e("items_producto_fk", 3, 4), e("productos_cat_fk", 4, 5), e("facturas_pedido_fk", 6, 2), e("pagos_factura_fk", 7, 6)] };
    },
    history_recent: () => [], saved_list: () => [], bookmarks_list: () => [], snippets_list: () => [],
  };
  window.__calls = [];
  window.__MOCK = handlers;
  let cb = 1;
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main", windowLabel: "main" } },
    transformCallback: (fn) => { const id = cb++; window["_cb" + id] = fn; return id; },
    unregisterCallback: () => {},
    convertFileSrc: (p) => p,
    invoke: async (cmd, args) => {
      window.__calls.push(cmd);
      if (cmd.startsWith("plugin:")) return null;
      const h = window.__MOCK[cmd];
      if (h) return h(args ?? {});
      return null;
    },
  };
})();
