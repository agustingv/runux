# Runux

Lanzador de juegos de Windows para Linux hecho con Tauri 2 + React.

Instala y abre Battle.net y la EA app, cada una en su propio prefijo de Wine o Proton,
detecta los juegos que instalan y los lanza desde la app. También añade juegos sueltos
(ejecutables de Windows, o juegos de emuladores: ScummVM, Dreamcast con Flycast, PlayStation
con DuckStation, PlayStation 2 con PCSX2, PlayStation 3 con RPCS3 y PSP con PPSSPP) e instala
desde ISO o DVD.

Proton no necesita Steam: la app puede descargar GE-Proton desde Ajustes.

## Desarrollo

```sh
npm install
npm run tauri dev
```

Requisitos: Rust, las dependencias de sistema de Tauri (webkit2gtk-4.1) y Proton o Wine de 64 bits.

## Emuladores

Cada emulador está en `src-tauri/src/emulator/` e implementa el trait `Emulator`: cómo se
detecta e instala (`Program`: en el `$PATH` o de Flathub, y a un Flatpak se le da acceso de
lectura a la carpeta del juego), qué abre (una imagen de disco o carpeta, o los archivos del
juego), qué hay que configurar en él antes de jugar, cómo lanza un juego y qué ajustes
arreglan problemas conocidos. La interfaz lo muestra todo a partir de
esa información: el apartado de su sistema en la barra lateral, su opción al añadir un juego
y sus interruptores en «Solución de problemas». Para añadir uno: una variante en
`external::Kind`, un módulo que implemente `Emulator` y su entrada en `emulator::ALL`.

## Rutas

- Configuración: `~/.config/runux/`
- Prefijos, GE-Proton y juegos: `~/.local/share/runux/`
- Caché (instaladores): `~/.cache/runux/`
- Registros de Proton (`PROTON_LOG=1`): `~/.cache/runux/logs/`, con una carpeta por juego

Las carpetas de la versión anterior (`btn`) se mueven solas al primer arranque y queda un enlace en su lugar.

## Publicar una versión

1. Sube la versión en `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` y `package.json`.
2. Haz commit y crea la etiqueta: `git tag v0.2.0 && git push origin main v0.2.0`.

El flujo `.github/workflows/release.yml` compila el `.deb` y el `.rpm`, los publica en una
GitHub Release y actualiza la página de descarga en GitHub Pages. La página es la plantilla
`site/index.html`, que `site/render.py` rellena con la versión, los enlaces, tamaños y SHA-256.

## Actualizaciones

Los paquetes instalan un repositorio que se publica en GitHub Pages con cada versión, así
que Runux se actualiza con el resto del sistema (`apt upgrade`, `dnf upgrade` o el
actualizador gráfico):

- APT: `/etc/apt/sources.list.d/runux.sources`, con la clave en `/usr/share/keyrings/runux.gpg`.
- DNF: `/etc/yum.repos.d/runux.repo`, con la clave en `/etc/pki/rpm-gpg/RPM-GPG-KEY-runux`.

Estos archivos están en `src-tauri/packaging/`. `site/repos.sh` crea los repositorios y
los firma con la clave privada guardada en el secreto `REPO_SIGNING_KEY` del repositorio
de GitHub (exportada con `gpg --export-secret-keys --armor`). Si cambias la clave, cambia
también las públicas de `src-tauri/packaging/`.
