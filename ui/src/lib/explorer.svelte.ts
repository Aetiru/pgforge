import {
  connect as ipcConnect,
  disconnect as ipcDisconnect,
  describeError,
  folderOf,
  listProfiles,
  renameGroup as ipcRenameGroup,
  saveProfile,
  treeChildren,
  treeSearch,
  type ConnectionProfile,
  type FolderKind,
  type SearchHit,
  type ServerCaps,
  type TreeNode,
  type TreeOptions,
} from "./ipc";
import { folderForKind } from "./tree-actions";

/**
 * Qué representa una fila del árbol.
 *
 * Los servidores son las raíces del mismo árbol, estén conectados o no. Tener una lista de
 * servidores aparte del árbol obligaba a entender dos cosas para hacer una: elegir el servidor en
 * un lado y navegarlo en el otro.
 *
 * Las carpetas (`group`) son locales: agrupan servidores guardados y no existen en ninguna base.
 * Por eso no se cargan del servidor y sus hijos están siempre ahí.
 */
export type RowKind = "group" | "server" | "node";

export interface Row {
  kind: RowKind;
  key: string;
  /** Vacío en las carpetas: no pertenecen a ningún servidor. */
  profileId: string;
  /** El nombre de la carpeta si la fila es una, o la carpeta que contiene al servidor. */
  group?: string;
  /** `null` en las carpetas y en los servidores: son filas locales, no objetos del catálogo. */
  node: TreeNode | null;
  level: number;
  label: string;
  detail?: string;
  comment?: string;
  hasChildren: boolean;
  expanded: boolean;
  loading: boolean;
  error?: string;
  children: Row[] | null;
  /** Solo significa algo en las filas de servidor. */
  connected: boolean;
}

/**
 * La segunda línea de un servidor. Lleva el usuario y no solo el host: dos perfiles contra el mismo
 * servidor con roles distintos —uno de solo lectura, otro de mantenimiento— son idénticos sin él.
 */
function serverDetail(profile: ConnectionProfile): string {
  return `${profile.user}@${profile.host}:${profile.port}`;
}

function serverRow(profile: ConnectionProfile, level: number): Row {
  return {
    kind: "server",
    key: profile.id,
    profileId: profile.id,
    group: profile.group,
    node: null,
    level,
    label: profile.name,
    detail: serverDetail(profile),
    hasChildren: true,
    expanded: false,
    loading: false,
    children: null,
    connected: false,
  };
}

/** Cuántos servidores tiene la carpeta y cuántos están conectados, sin abrirla. */
function groupDetail(servers: Row[]): string {
  // Una carpeta recién creada todavía no tiene nada: decirlo con palabras es más claro que un «0»
  // y le explica al usuario que ahí adentro se arrastran servidores.
  if (servers.length === 0) return "vacía";
  const connected = servers.filter((server) => server.connected).length;
  return connected > 0
    ? `${servers.length} · ${connected} conectado${connected === 1 ? "" : "s"}`
    : `${servers.length}`;
}

/**
 * Una sección de servidores. Con `name` en `null` es la de los que no están en ninguna carpeta.
 *
 * Esa sección existe para que soltar un servidor fuera de toda carpeta tenga **dónde** soltarse:
 * antes el destino era el fondo del panel, que no se ve y que encima cambia de tamaño con la
 * cantidad de filas. Solo aparece si hay al menos una carpeta: sin ninguna, «sin carpeta» sería el
 * título de la lista entera.
 */
const LOOSE_LABEL = "Sin carpeta";

function groupRow(name: string | null, servers: Row[], expanded: boolean): Row {
  return {
    kind: "group",
    key: `g:${name ?? ""}`,
    profileId: "",
    group: name ?? undefined,
    node: null,
    level: 0,
    label: name ?? LOOSE_LABEL,
    detail: groupDetail(servers),
    hasChildren: servers.length > 0,
    expanded,
    loading: false,
    children: servers,
    connected: false,
  };
}

function childRow(parent: Row, node: TreeNode): Row {
  return {
    kind: "node",
    key: `${parent.profileId}:${node.id}`,
    profileId: parent.profileId,
    node,
    level: parent.level + 1,
    label: node.label,
    detail: node.detail,
    comment: node.comment,
    hasChildren: node.hasChildren,
    expanded: false,
    loading: false,
    children: null,
    connected: true,
  };
}

const byName = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: "base" });

/** Los cajones donde puede estar una relación, en el orden en que conviene mirarlos. */
const RELATION_FOLDERS: FolderKind[] = ["tables", "views", "materializedViews", "foreignTables"];

class Explorer {
  profiles = $state<ConnectionProfile[]>([]);
  caps = $state<Record<string, ServerCaps>>({});
  roots = $state<Row[]>([]);
  selected = $state<Row | null>(null);
  options = $state<TreeOptions>({ showSystemSchemas: false });
  search = $state("");

  /**
   * Deja fuera del árbol a los servidores sin conectar. No los desconfigura ni los borra: con veinte
   * conexiones guardadas, las tres que están en uso quedan sepultadas entre las diecisiete que no.
   */
  onlyConnected = $state(false);

  /**
   * El resultado de la última búsqueda contra el servidor, que reemplaza al árbol mientras está.
   * `null` = no se buscó nada y se está viendo el árbol; una lista vacía = se buscó y no hubo nada.
   */
  hits = $state.raw<SearchHit[] | null>(null);
  searching = $state(false);
  searchError = $state<string | null>(null);
  /** Contra qué servidor se buscó, para poder revelar lo que se elija del resultado. */
  private hitProfile: string | null = null;

  /**
   * Carpetas creadas en la interfaz que todavía no tienen ningún servidor. Viven solo acá, en
   * memoria: como una carpeta es un nombre compartido entre perfiles y no hay lista de carpetas
   * guardada aparte, una que quede vacía no tiene dónde persistir y desaparece al reiniciar. En
   * cuanto se le arrastra el primer servidor pasa a existir de verdad y sale de esta lista.
   */
  pendingGroups = $state<string[]>([]);

  /** Todas las filas de servidor, estén dentro de una carpeta o sueltas. */
  get servers(): Row[] {
    return this.roots.flatMap((row) => (row.kind === "group" ? (row.children ?? []) : [row]));
  }

  /** Las carpetas existentes, para ofrecerlas al elegir dónde va un servidor. */
  get groups(): string[] {
    // La sección de los servidores sueltos es una fila de carpeta sin nombre: no es una carpeta a
    // la que se pueda mandar nada, es la ausencia de carpeta.
    return this.roots
      .filter((row) => row.kind === "group" && row.group !== undefined)
      .map((row) => row.group!);
  }

  /** Relee los perfiles guardados y vuelve a armar el árbol con ellos. */
  async refreshProfiles() {
    this.profiles = await listProfiles();
    this.rebuild();
  }

  /**
   * Crea una carpeta vacía y la deja seleccionada. No toca el disco: recién persiste cuando se le
   * arrastra un servidor adentro (ver [`pendingGroups`]). Si ya existe una carpeta con ese nombre,
   * no hace nada —quien la llama valida antes y avisa—.
   */
  newGroup(name: string) {
    const trimmed = name.trim();
    if (!trimmed) return;
    const exists =
      this.profiles.some((profile) => profile.group === trimmed) ||
      this.pendingGroups.includes(trimmed);
    if (exists) return;

    this.pendingGroups.push(trimmed);
    this.rebuild();
    const row = this.roots.find((item) => item.kind === "group" && item.group === trimmed);
    if (row) this.selected = row;
  }

  /**
   * Arma las raíces del árbol a partir de los perfiles y de las carpetas vacías pendientes, sin
   * perder lo que ya estaba abierto: las filas de servidor se reutilizan tal cual, así que mover un
   * servidor de carpeta no lo desconecta ni descarta lo que ya se cargó de él.
   */
  private rebuild() {
    const profiles = this.profiles;

    const previous = new Map(this.servers.map((row) => [row.profileId, row]));
    // Por clave y no por nombre: la sección de los sueltos no tiene nombre.
    const previousGroups = new Map(
      this.roots.filter((row) => row.kind === "group").map((row) => [row.key, row]),
    );

    const rowOf = (profile: ConnectionProfile, level: number) => {
      const existing = previous.get(profile.id);
      if (!existing) return serverRow(profile, level);
      existing.label = profile.name;
      existing.detail = serverDetail(profile);
      existing.group = profile.group;
      existing.level = level;
      return existing;
    };

    // Las carpetas que siguen existiendo se reutilizan tal cual: si no, cada guardado de un perfil
    // las cerraría y dejaría la selección apuntando a una fila que ya no está en el árbol.
    const groupOf = (name: string | null, servers: Row[]) => {
      const existing = previousGroups.get(`g:${name ?? ""}`);
      if (!existing) return groupRow(name, servers, true);
      existing.children = servers;
      existing.detail = groupDetail(servers);
      existing.hasChildren = servers.length > 0;
      return existing;
    };

    const sorted = [...profiles].sort((a, b) => byName(a.name, b.name));
    const grouped = new Map<string, ConnectionProfile[]>();
    const loose: ConnectionProfile[] = [];
    for (const profile of sorted) {
      if (profile.group) {
        const list = grouped.get(profile.group);
        if (list) list.push(profile);
        else grouped.set(profile.group, [profile]);
      } else {
        loose.push(profile);
      }
    }

    // Una carpeta vacía pendiente que acaba de recibir un servidor ya existe de verdad entre los
    // perfiles: se saca de la lista para no mostrarla dos veces.
    this.pendingGroups = this.pendingGroups.filter((name) => !grouped.has(name));

    // Las carpetas van arriba y los servidores sueltos abajo, como en cualquier explorador de
    // archivos. Una carpeta nueva arranca abierta: se acaba de crear para poner algo adentro. Las
    // pendientes se intercalan por nombre con las que tienen servidores, y salen vacías.
    //
    // Un servidor de carpeta se queda en el nivel 0, igual que uno suelto: la carpeta se dibuja como
    // encabezado de sección y ya agrupa a la vista, así que sangrar además cobra dos veces el mismo
    // ancho —y el ancho es lo que le falta a los nombres del final del árbol—.
    const groupNames = [...grouped.keys(), ...this.pendingGroups].sort(byName);
    const looseRows = loose.map((profile) => rowOf(profile, 0));
    this.roots = [
      ...groupNames.map((name) =>
        groupOf(name, (grouped.get(name) ?? []).map((profile) => rowOf(profile, 0))),
      ),
      // Con carpetas, los sueltos van bajo su propio rótulo: es el destino visible para sacar un
      // servidor de una carpeta. Sin ninguna carpeta no hace falta, porque no hay nada de qué
      // distinguirlos.
      ...(groupNames.length > 0 && looseRows.length > 0 ? [groupOf(null, looseRows)] : looseRows),
    ];

    // Una carpeta renombrada o vaciada deja de existir: seguir mostrándola en el detalle sería
    // mostrar algo que ya no está en el árbol.
    if (this.selected?.kind === "group" && !this.roots.includes(this.selected)) {
      this.selected = null;
    }
  }

  rowFor(profileId: string): Row | null {
    return this.servers.find((row) => row.profileId === profileId) ?? null;
  }

  isConnected(profileId: string) {
    return this.rowFor(profileId)?.connected ?? false;
  }

  /** Mueve un servidor a una carpeta, o lo saca de la suya si `group` es `null`. */
  async moveToGroup(profileId: string, group: string | null) {
    const profile = this.profiles.find((item) => item.id === profileId);
    if (!profile || (profile.group ?? null) === group) return;

    // Si este era el último servidor de su carpeta, esa carpeta va a quedar vacía. Se la conserva
    // como carpeta pendiente en vez de dejar que desaparezca: el usuario no la borró, solo sacó algo
    // de adentro, y verla esfumarse sola es desconcertante. Sigue la misma regla que cualquier
    // carpeta vacía —no persiste al reiniciar—.
    const from = profile.group ?? null;
    if (from && !this.profiles.some((item) => item.id !== profileId && item.group === from)) {
      if (!this.pendingGroups.includes(from)) this.pendingGroups.push(from);
    }

    // Sin contraseña: el comando deja intacta la que ya esté guardada.
    await saveProfile({ ...$state.snapshot(profile), group: group ?? undefined });
    await this.refreshProfiles();
  }

  /** Renombra una carpeta, o la deshace si no se pasa nombre nuevo. */
  async renameGroup(from: string, to?: string) {
    await ipcRenameGroup(from, to);
    await this.refreshProfiles();
  }

  async connect(profile: ConnectionProfile, password?: string, trustHostKey?: boolean) {
    const result = await ipcConnect(profile.id, password, undefined, trustHostKey);
    this.caps[profile.id] = result.caps;

    const row = this.rowFor(profile.id);
    if (row) {
      row.connected = true;
      row.error = undefined;
      row.children = null;
      await this.toggle(row);
      this.refreshGroupDetail(row);
    }
  }

  async disconnect(profileId: string) {
    await ipcDisconnect(profileId);
    const row = this.rowFor(profileId);
    if (row) {
      row.connected = false;
      row.expanded = false;
      row.children = null;
      this.refreshGroupDetail(row);
    }
    delete this.caps[profileId];
    if (this.selected?.profileId === profileId) {
      this.selected = null;
    }
  }

  /** Vuelve a contar los conectados de la carpeta que contiene a `row`, si está en una. */
  private refreshGroupDetail(row: Row) {
    const group = this.roots.find((item) => item.kind === "group" && item.group === row.group);
    if (group?.children) group.detail = groupDetail(group.children);
  }

  /** `true` si la fila no se puede abrir todavía porque su servidor está desconectado. */
  needsConnection(row: Row) {
    return row.kind === "server" && !row.connected;
  }

  async toggle(row: Row) {
    if (!row.hasChildren || this.needsConnection(row)) return;

    if (row.expanded) {
      row.expanded = false;
      return;
    }
    if (row.children === null) {
      await this.loadChildren(row);
    }
    row.expanded = true;
  }

  async loadChildren(row: Row) {
    row.loading = true;
    row.error = undefined;
    try {
      const nodes = await treeChildren(row.profileId, row.node, this.options);
      row.children = nodes.map((node) => childRow(row, node));
    } catch (error) {
      // El nodo queda expandido y vacío con el motivo a la vista, en vez de fallar en silencio.
      row.error = describeError(error);
      row.children = [];
    } finally {
      row.loading = false;
    }
  }

  async reload(row: Row) {
    // Una carpeta de conexiones no se lee de ningún servidor: no hay nada que releer.
    if (row.kind === "group") return;
    row.children = null;
    if (row.expanded) {
      await this.loadChildren(row);
    }
  }

  /**
   * Relee un nodo y todo lo que tenga abierto debajo, dejando el árbol como estaba.
   *
   * `reload` es «traeme esto de nuevo» y por eso descarta lo de adentro. Después de un DDL hace
   * falta lo contrario: el usuario está mirando una lista de tablas y lo que espera es ver aparecer
   * la que acaba de crear, no que se le cierre el camino que abrió. Se releen solo las ramas
   * abiertas, así que cuesta una consulta por nodo a la vista y ninguna por lo que está cerrado.
   */
  private async refreshOpen(row: Row) {
    // Nada cargado: se va a leer solo cuando se lo abra, y ahí ya va a venir al día.
    if (row.children === null) return;

    const open = new Map(
      row.children.filter((child) => child.expanded).map((child) => [child.key, child]),
    );
    await this.loadChildren(row);

    for (const child of row.children ?? []) {
      const previous = open.get(child.key);
      if (!previous) continue;

      child.expanded = true;
      // Lo que tenía cargado el nodo viejo se le presta al nuevo solo para que `refreshOpen` sepa
      // que hay algo que releer; la llamada de abajo lo reemplaza por lo que diga el servidor.
      child.children = previous.children;
      await this.refreshOpen(child);
    }

    // La selección apunta a una fila que se acaba de reemplazar: sin volver a apuntarla, el panel
    // de detalle sigue mostrando el objeto viejo.
    const selected = this.selected;
    if (selected) {
      const fresh = row.children?.find((child) => child.key === selected.key);
      if (fresh) this.selected = fresh;
    }
  }

  /**
   * Pone al día lo que esté abierto de un servidor. Lo llama la pestaña de consulta después de un
   * DDL: el editor y el árbol miran el mismo servidor por conexiones distintas, y hasta ahora una
   * tabla borrada desde el editor seguía en el árbol hasta refrescar a mano.
   *
   * Se refresca el servidor entero y no la base donde se corrió: `CREATE ROLE` y `CREATE DATABASE`
   * no cuelgan de ninguna base, y acertar el nodo exacto por el texto del SQL es justo lo que no se
   * puede hacer bien.
   */
  async refreshServer(profileId: string) {
    const server = this.rowFor(profileId);
    if (!server?.connected || !server.expanded) return;
    await this.refreshOpen(server);
  }

  /**
   * Busca objetos por nombre contra el catálogo del servidor.
   *
   * El filtro de la barra recorre lo que ya se trajo, que es justo lo que uno ya había abierto y no
   * necesita buscar. Esto va al servidor y trae también lo que el árbol nunca abrió.
   */
  async searchServer(profileId: string, database: string, pattern: string) {
    this.hits = null;
    this.searchError = null;
    if (pattern.trim() === "") return;

    this.searching = true;
    this.hitProfile = profileId;
    try {
      this.hits = await treeSearch(profileId, database, pattern, this.options);
    } catch (error) {
      this.searchError = describeError(error);
      this.hits = [];
    } finally {
      this.searching = false;
    }
  }

  /** Vuelve del resultado de la búsqueda al árbol. */
  clearHits() {
    this.hits = null;
    this.searchError = null;
    this.hitProfile = null;
  }

  /**
   * Abre el camino hasta una coincidencia y cierra el resultado: elegir uno es haber terminado de
   * buscar. Si el objeto ya no está —lo borró otro entre la búsqueda y el clic—, el árbol queda
   * parado en su esquema, que sigue siendo un lugar razonable.
   */
  async revealHit(hit: SearchHit): Promise<Row | null> {
    const profileId = this.hitProfile;
    if (!profileId) return null;

    const folder = folderForKind(hit.kind);
    if (!folder) return null;

    const row = await this.revealObject(profileId, hit.database, hit.schema, hit.oid, folder);
    this.clearHits();
    return row;
  }

  /** Baja un escalón: deja `row` leído y abierto, y devuelve el hijo que se buscaba. */
  private async descend(row: Row, match: (node: TreeNode) => boolean): Promise<Row | null> {
    if (row.children === null) await this.loadChildren(row);
    row.expanded = true;
    return row.children?.find((child) => child.node !== null && match(child.node)) ?? null;
  }

  /** Abre el camino hasta una base y la deja elegida. `null` si el servidor no está conectado. */
  async revealDatabase(profileId: string, database: string): Promise<Row | null> {
    const server = this.rowFor(profileId);
    if (!server?.connected) return null;

    const row = await this.descend(
      server,
      (node) => node.kind === "database" && node.label === database,
    );
    if (row) this.selected = row;
    return row;
  }

  /**
   * Abre el camino hasta una relación y la deja elegida.
   *
   * Los cajones de relaciones se prueban en orden en vez de ir directo al que corresponde: lo único
   * que lleva encima una pestaña de datos es el OID, no de qué tipo es el objeto. Abrir los cuatro
   * de una serían cuatro consultas para encontrar algo que casi siempre está en «Tablas»; el que no
   * tenía nada se vuelve a cerrar, para no dejar el esquema entero desplegado por una búsqueda.
   */
  async revealRelation(
    profileId: string,
    database: string,
    schema: string,
    oid: number,
  ): Promise<Row | null> {
    const schemaRow = await this.revealSchema(profileId, database, schema);
    if (!schemaRow) return null;

    for (const kind of RELATION_FOLDERS) {
      const found = await this.openFolder(schemaRow, kind, oid);
      if (found) return found;
    }

    // Se llegó hasta el esquema pero el objeto no está: pudo borrarlo otro, o quedar escondido por
    // las opciones del árbol. Dejarlo elegido es mejor que no mover nada y parecer que falló.
    this.selected = schemaRow;
    return null;
  }

  /**
   * Lo mismo, sabiendo en qué carpeta vive el objeto. Es el caso de la búsqueda contra el servidor,
   * que devuelve el tipo: se baja derecho al cajón en vez de probar cuatro.
   */
  async revealObject(
    profileId: string,
    database: string,
    schema: string,
    oid: number,
    folder: FolderKind,
  ): Promise<Row | null> {
    const schemaRow = await this.revealSchema(profileId, database, schema);
    if (!schemaRow) return null;

    const found = await this.openFolder(schemaRow, folder, oid);
    if (found) return found;

    this.selected = schemaRow;
    return null;
  }

  /** Abre el camino hasta un esquema: base, carpeta «Esquemas» y el esquema mismo. */
  private async revealSchema(
    profileId: string,
    database: string,
    schema: string,
  ): Promise<Row | null> {
    const databaseRow = await this.revealDatabase(profileId, database);
    if (!databaseRow) return null;

    const schemas = await this.descend(databaseRow, (node) => folderOf(node.kind) === "schemas");
    if (!schemas) return null;

    return await this.descend(schemas, (node) => node.kind === "schema" && node.label === schema);
  }

  /**
   * Busca el objeto en una carpeta del esquema. Si no está, la vuelve a cerrar: probar cuatro
   * cajones no tiene por qué dejar el esquema entero desplegado.
   */
  private async openFolder(
    schemaRow: Row,
    kind: FolderKind,
    oid: number,
  ): Promise<Row | null> {
    const folder = await this.descend(schemaRow, (node) => folderOf(node.kind) === kind);
    if (!folder) return null;

    const found = await this.descend(folder, (node) => node.oid === oid);
    if (found) {
      this.selected = found;
      return found;
    }
    folder.expanded = false;
    return null;
  }

  /** Descarta lo cargado de los servidores conectados; se usa al cambiar las opciones. */
  async reloadAll() {
    for (const server of this.servers) {
      if (!server.connected) continue;
      server.children = null;
      server.expanded = false;
      await this.toggle(server);
    }
    this.selected = null;
  }

  /**
   * Cierra todo lo que esté abierto sin desconectar nada ni descartar lo cargado: volver a abrir un
   * servidor no le vuelve a pedir el catálogo. Las carpetas de conexiones quedan abiertas, porque
   * cerrarlas escondería justamente los servidores que se quieren volver a ver.
   */
  collapseAll() {
    const walk = (list: Row[]) => {
      for (const row of list) {
        if (row.kind !== "group") row.expanded = false;
        if (row.children) walk(row.children);
      }
    };
    walk(this.roots);
  }

  select(row: Row) {
    this.selected = row;
  }
}

export const explorer = new Explorer();

/**
 * Si la fila sobrevive al filtro de «solo conectados».
 *
 * Se filtran servidores y no nodos: todo lo que cuelga de un servidor conectado lo está. Una carpeta
 * se va con sus servidores, porque un encabezado sin nada adentro ocupa lo mismo que uno con algo.
 * El filtro no descarta las filas del árbol, solo deja de dibujarlas: las que se esconden conservan
 * lo que tenían abierto y cargado para cuando se lo apague.
 */
function passesFilter(row: Row, onlyConnected: boolean): boolean {
  if (!onlyConnected) return true;
  if (row.kind === "server") return row.connected;
  if (row.kind === "group") return (row.children ?? []).some((server) => server.connected);
  return true;
}

/**
 * Aplana el árbol a la lista de filas visibles.
 *
 * Con búsqueda activa se muestran las coincidencias y sus ancestros, sin importar si el nodo
 * estaba expandido. Solo alcanza a lo que ya se trajo del servidor: el árbol se carga por niveles
 * y buscar en todo el catálogo sería otra cosa.
 */
export function visibleRows(rows: Row[], search = "", onlyConnected = false): Row[] {
  const needle = search.trim().toLowerCase();
  const out: Row[] = [];

  if (needle === "") {
    const walk = (list: Row[]) => {
      for (const row of list) {
        if (!passesFilter(row, onlyConnected)) continue;
        out.push(row);
        if (row.expanded && row.children) walk(row.children);
      }
    };
    walk(rows);
    return out;
  }

  const collect = (list: Row[], into: Row[]) => {
    for (const row of list) {
      if (!passesFilter(row, onlyConnected)) continue;

      const matched: Row[] = [];
      if (row.children) collect(row.children, matched);

      if (matched.length > 0 || row.label.toLowerCase().includes(needle)) {
        into.push(row, ...matched);
      }
    }
  };
  collect(rows, out);
  return out;
}
