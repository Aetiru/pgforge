/**
 * La RAM del servidor, que el usuario escribe una vez por servidor: es lo único que la revisión de
 * configuración necesita y no puede preguntarle al catálogo. Preferencia de la aplicación, no del
 * perfil, como las alertas: guardarla en `connections.json` obligaría a migrar el perfil.
 */
const key = (profileId: string) => `pgforge.ram.${profileId}`;

export function loadRam(profileId: string): string {
  try {
    return localStorage.getItem(key(profileId)) ?? "";
  } catch {
    return "";
  }
}

export function saveRam(profileId: string, gigabytes: string): void {
  try {
    if (gigabytes.trim() === "") localStorage.removeItem(key(profileId));
    else localStorage.setItem(key(profileId), gigabytes.trim());
  } catch {
    // Sin almacenamiento el dato se pide de nuevo en la próxima sesión; no vale la pena avisar.
  }
}
