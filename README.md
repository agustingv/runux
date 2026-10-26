# Runux

Lanzador de juegos de Windows para Linux hecho con Tauri 2 + React.

Instala y abre Battle.net y la EA app, cada una en su propio prefijo de Wine o Proton,
detecta los juegos que instalan y los lanza desde la app. También añade juegos sueltos
(ejecutables de Windows, ScummVM) e instala desde ISO o DVD.

Proton no necesita Steam: la app puede descargar GE-Proton desde Ajustes.

## Desarrollo

```sh
npm install
npm run tauri dev
```

Requisitos: Rust, las dependencias de sistema de Tauri (webkit2gtk-4.1) y Proton o Wine de 64 bits.

## Rutas

- Configuración: `~/.config/runux/`
- Prefijos, GE-Proton y juegos: `~/.local/share/runux/`
- Caché (instaladores): `~/.cache/runux/`

Las carpetas de la versión anterior (`btn`) se mueven solas al primer arranque y queda un enlace en su lugar.

## Publicar una versión

1. Sube la versión en `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` y `package.json`.
2. Haz commit y crea la etiqueta: `git tag v0.2.0 && git push origin main v0.2.0`.

El flujo `.github/workflows/release.yml` compila el `.deb` y el `.rpm`, los publica en una
GitHub Release y actualiza la página de descarga en GitHub Pages. La página es la plantilla
`site/index.html`, que `site/render.py` rellena con la versión, los enlaces, tamaños y SHA-256.
