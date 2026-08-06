import {
  connect as ipcConnect,
  disconnect as ipcDisconnect,
  describeError,
  listProfiles,
  renameGroup as ipcRenameGroup,
  saveProfile,
  treeChildren,
  type ConnectionProfile,
  type ServerCaps,
  type TreeNode,
  type TreeOptions,
} from "./ipc";

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

function serverRow(profile: ConnectionProfile, level: number): Row {
  return {
    kind: "server",
    key: profile.id,
    profileId: profile.id,
    group: profile.group,
    node: null,
    level,
    label: profile.name,
    detail: `${profile.host}:${profile.port}`,
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

function groupRow(name: string, servers: Row[], expanded: boolean): Row {
  return {
    kind: "group",
    key: `g:${name}`,
    profileId: "",
    group: name,
    node: null,
    level: 0,
    label: name,
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
    return this.roots.filter((row) => row.kind === "group").map((row) => row.group!);
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
    const previousGroups = new Map(
      this.roots.filter((row) => row.kind === "group").map((row) => [row.group!, row]),
    );

    const rowOf = (profile: ConnectionProfile, level: number) => {
      const existing = previous.get(profile.id);
      if (!existing) return serverRow(profile, level);
      existing.label = profile.name;
      existing.detail = `${profile.host}:${profile.port}`;
      existing.group = profile.group;
      existing.level = level;
      return existing;
    };

    // Las carpetas que siguen existiendo se reutilizan tal cual: si no, cada guardado de un perfil
    // las cerraría y dejaría la selección apuntando a una fila que ya no está en el árbol.
    const groupOf = (name: string, servers: Row[]) => {
      const existing = previousGroups.get(name);
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
    const groupNames = [...grouped.keys(), ...this.pendingGroups].sort(byName);
    this.roots = [
      ...groupNames.map((name) =>
        groupOf(name, (grouped.get(name) ?? []).map((profile) => rowOf(profile, 1))),
      ),
      ...loose.map((profile) => rowOf(profile, 0)),
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
