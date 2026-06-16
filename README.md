# 🦀 neoxagent

> **Agente de nodo ligero y seguro para Jexactyl escrito en Rust con Podman nativo.**
>
> Diseñado como una alternativa moderna, eficiente, daemonless y rootless a *Pterodactyl Wings*, optimizado para integrarse con el panel *Jexactyl* (basado en Next.js).

---

## 📐 Descripción General y Arquitectura

`neoxagent` se ejecuta en cada nodo de la infraestructura de hosting y expone una API REST y WebSocket segura (autenticada mediante un Bearer API Key). El agente se comunica directamente con el socket local de **Podman**, eliminando la necesidad de un daemon en ejecución persistente (como `dockerd` en Docker) y ejecutándose en modo **rootless** (sin privilegios de root) por diseño para maximizar la seguridad del host.

### Stack Tecnológico
*   **API Web y Enrutamiento:** `axum` (asíncrono y de alto rendimiento).
*   **WebSockets:** `axum` + `tokio-tungstenite` para logs, consola interactiva y telemetría en tiempo real.
*   **Runtime Asíncrono:** `tokio`.
*   **Integración con Contenedores:** `podman-api` (SDK nativo de Podman).
*   **Serialización y Estructuración:** `serde`, `serde_json`, `serde_yaml`, y `toml`.
*   **Seguridad:** Middleware de autenticación Bearer Token custom, validación estricta de Path Traversal.
*   **Compresión y Criptografía:** `tar`, `flate2` y `sha2` para la gestión de respaldos.

---

## 🛠️ Características Detalladas del Proyecto

El proyecto está estructurado alrededor de un roadmap de 7 fases funcionales, todas implementadas en el código actual:

### 1. Sistema y Estado del Host
*   **Endpoints de Salud:** `/api/health` para comprobar la disponibilidad del agente y su conectividad con Podman.
*   **Información del Sistema:** `/api/system/info` que retorna detalles del host (OS, arquitectura, núcleos de CPU, RAM total, versión de Podman y cgroups).
*   **Métricas en Tiempo Real:** `/api/system/resources` para telemetría general de consumo de CPU, RAM y uso de almacenamiento en el nodo.

### 2. Gestión de Contenedores (CRUD & Lifecycle)
*   **Operaciones CRUD:** Listar, inspeccionar, crear y eliminar contenedores gestionados por el agente.
*   **Ciclo de vida:** Métodos HTTP para iniciar (`/start`), detener de forma ordenada con timeout (`/stop`), reiniciar (`/restart`) y forzar la detención (`/kill`) de los procesos.
*   **Límites de Recursos:** Configuración de cuotas de CPU (cores), límites estrictos de memoria RAM y cuotas de disco activas mediante el kernel (ext4/XFS project quotas).

### 3. Gestión de Pods y Redes (Netavark)
*   **Gestión de Pods:** Agrupación lógica de contenedores que comparten la misma interfaz de red, dirección IP y localhost (comportamiento idéntico a los Pods de Kubernetes).
*   **Redes Netavark:** Endpoints CRUD para crear y administrar redes virtuales de contenedores aisladas del exterior.

### 4. Administrador de Archivos (File Manager)
*   **CRUD de Archivos:** Listar directorios, leer contenido de archivos de texto (límite de 10 MB para evitar saturar memoria), escribir/modificar archivos y crear directorios.
*   **Acciones avanzadas:** Renombrar y mover ficheros, eliminar directorios de forma recursiva.
*   **Subidas y Descargas:** Carga de archivos mediante Multipart Form Data (límite de 100 MB) y descarga de archivos o carpetas comprimidos en caliente como un stream `.tar.gz`.
*   **Seguridad Antitraversal:** Resolución y canonicalización estricta de rutas mediante un módulo de validación seguro (`safe_resolve`) que impide ataques de escape de directorio (`../`).

### 5. Copias de Seguridad (Backups)
*   **Respaldos Atómicos:** Compresión `.tar.gz` del volumen de datos del Pod, con detención temporal opcional del servidor para garantizar consistencia.
*   **Validación de Integridad:** Generación y verificación de sumas de comprobación SHA256 para cada backup.
*   **Rotación Automática:** Límites configurables de copias de seguridad por servidor (`max_per_server`), eliminando automáticamente las más antiguas al superar la cuota.
*   **Restauración:** Limpieza automática del volumen actual y extracción del respaldo para restaurar el estado previo de los datos.

### 6. Imágenes de Contenedor
*   **Operaciones:** Listado de imágenes locales del nodo y borrado de imágenes no utilizadas.
*   **Búsqueda y Descarga:** Buscar imágenes en registros públicos (Docker Hub, Quay) y descargarlas (`pull`).
*   **Stream de Progreso:** WebSocket dedicado que reporta en tiempo real el progreso de descarga de las capas de la imagen.

### 7. Integración con Systemd
*   **Persistencia al Boot:** Generación dinámica de unidades systemd (`.service`) en caliente basadas en el contenedor o Pod de Podman.
*   **Modo Rootless y Root:** Detección automática del nivel de privilegios del agente para ubicar los archivos en `/etc/systemd/system` o en directorios del usuario (`~/.config/systemd/user`).
*   **Operaciones de Servicio:** Habilitar, deshabilitar y verificar el estado actual del servicio systemd para que los servidores arranquen de forma autónoma con el host.

---

## 🚫 Características Desactivadas Temporalmente

Para estabilizar la arquitectura básica y optimizar la comunicación del panel, las siguientes características avanzadas de red y orquestación se encuentran **desactivadas o marcadas como no disponibles temporalmente** en la interfaz e integración de producción:

1.  **Proxy de Red (Tun2socks):**
    *   *Propósito:* Creación de contenedores sidecar dentro del Pod del servidor de juegos para forzar que todo su tráfico sea ruteado a través de un proxy SOCKS5/VPN.
    *   *Estado:* La lógica de red en [pods.rs](file:///c:/proyectos/NeoxAgent/src/routes/pods.rs) está presente para pruebas, pero deshabilitada de cara a la producción hasta optimizar la estabilidad de los túneles y las latencias de conexión.
2.  **Pilas Multicontenedor (Kubernetes YAML):**
    *   *Propósito:* Levantamiento de arquitecturas compuestas mediante la traducción y ejecución directa de archivos de manifiesto YAML con `podman play kube` (el reemplazo natural de Docker Compose en Podman).
    *   *Estado:* Los endpoints en [kube.rs](file:///c:/proyectos/NeoxAgent/src/routes/kube.rs) están pausados temporalmente debido a la necesidad de refinar el control de permisos del host para volúmenes persistentes multicontenedor.

---

## ⚠️ Características Faltantes (Diferencias con Pterodactyl Wings)

Comparado con el agente oficial de Pterodactyl (*Wings*), `neoxagent` omite o tiene pendientes de desarrollo las siguientes funcionalidades clave:

1.  **Servidor SFTP Incorporado (SFTP Server Daemon):**
    *   *Wings:* Incluye un servidor SFTP nativo corriendo en Go (usualmente en el puerto 2022) que permite a los usuarios gestionar sus archivos remotamente usando clientes como FileZilla o WinSCP usando las mismas credenciales de Jexactyl.
    *   *neoxagent:* Solo expone operaciones de archivos a través de endpoints REST HTTP de la API, lo que obliga al panel a servir como intermediario de todas las interacciones de archivos o requiere configurar un servidor SFTP secundario en el host.
2.  **Watchdog Local y Monitoreo de Caídas (Crash Detection Loop):**
    *   *Wings:* Mantiene un loop de monitorización interno constante que detecta si un servidor de juegos se ha detenido abruptamente (ej. Out of Memory o código de salida anormal) y aplica políticas dinámicas de reinicio inteligente.
    *   *neoxagent:* Depende de la política de reinicio básica de Podman o Systemd. Carece de un hilo centinela local en Rust con lógica condicional avanzada para evaluar caídas continuas y alertar al panel sobre bucles de reinicio.
3.  **Ejecución Autónoma de Tareas Programadas (Daemon-side Schedules/Cron):**
    *   *Wings:* Aloja un planificador de tareas interno que ejecuta crons locales (reinicios automáticos, backups, comandos de consola) de manera autónoma, incluso si el panel de control web principal se encuentra sin conexión.
    *   *neoxagent:* No tiene un programador de tareas en segundo plano. Toda automatización debe ser gatillada externamente mediante peticiones REST originadas por el panel.
4.  **Autenticación en Registros Privados (Private Docker Registries Auth):**
    *   *Wings:* Permite registrar y pasar credenciales personalizadas para repositorios de imágenes privados de Docker, permitiendo a cada servidor descargar imágenes no públicas de forma segura.
    *   *neoxagent:* Realiza descargas de imágenes públicas y asume que el host ya tiene preconfiguradas las credenciales de Podman para repositorios privados.
5.  **Motor Egg/Nest Parser (Config Parser Dinámico):**
    *   *Wings:* Cuenta con un procesador avanzado de archivos de configuración capaz de abrir, modificar y guardar propiedades en diversos formatos (XML, JSON, INI, YAML, archivos de texto plano) usando expresiones regulares e inyecciones dinámicas definidas en la configuración del servidor de juegos.
    *   *neoxagent:* Carece de un motor parseador de archivos específico de juegos. Las inyecciones de variables se realizan únicamente en el entorno del contenedor.
6.  **Historial y Almacenamiento de Estadísticas (Resource History/RRD):**
    *   *Wings:* Guarda un registro temporal interno de los últimos minutos/horas del consumo de recursos para poder dibujar gráficos de uso histórico en el momento en que el usuario entra al panel.
    *   *neoxagent:* Las estadísticas son efímeras; se transmiten directamente al cliente WebSocket activo y no se guardan en ninguna base de datos ni memoria temporal en el nodo.
7.  **Estado de Suspensión y Bloqueo Estricto (Server Suspension State):**
    *   *Wings:* Implementa un estado de suspensión que bloquea físicamente la lectura de archivos, impide el arranque del contenedor y deshabilita cualquier acción hasta que el panel envíe la orden de desbloqueo.
    *   *neoxagent:* No dispone de una bandera o middleware de suspensión que bloquee el acceso al sistema de archivos local de los servidores desactivados.
8.  **SSL Auto-Configuration (ACME / Let's Encrypt nativo):**
    *   *Wings:* Puede comunicarse de forma integrada con autoridades de certificación para obtener y renovar automáticamente los certificados TLS necesarios para la comunicación HTTPS.
    *   *neoxagent:* Requiere que los certificados TLS (si están habilitados) se gestionen de forma manual o externa a través de servicios como Certbot.

---

## 📂 Estructura del Código Fuente

El código se encuentra organizado de la siguiente manera:

*   [`src/main.rs`](file:///c:/proyectos/NeoxAgent/src/main.rs): Inicializador principal de la aplicación, configuración de logging, conexión al socket de Podman y declaración del router de Axum.
*   [`src/config.rs`](file:///c:/proyectos/NeoxAgent/src/config.rs): Estructura y lógica de carga del archivo `config.toml`.
*   [`src/auth.rs`](file:///c:/proyectos/NeoxAgent/src/auth.rs): Middleware de autenticación Bearer para validación de la API Key.
*   [`src/error.rs`](file:///c:/proyectos/NeoxAgent/src/error.rs): Enumeración `AppError` para centralizar y responder adecuadamente con estados HTTP ante fallos del sistema o de Podman.
*   [`src/models/`](file:///c:/proyectos/NeoxAgent/src/models/): Definición de esquemas de datos deserializables para requests y serializables para responses.
*   [`src/routes/`](file:///c:/proyectos/NeoxAgent/src/routes/): Controladores que manejan la lógica de los endpoints HTTP y WebSockets.
*   [`src/services/`](file:///c:/proyectos/NeoxAgent/src/services/): Lógica intermedia y comunicación directa con la SDK de Podman.

---

## 🚀 Instalación y Pruebas en un VPS

### Requisitos del Host
- **SO**: Debian 11/12 o Ubuntu 20.04/22.04 (x86_64).
- **Acceso**: Privilegios de `root` (para la instalación).
- **Dependencias**: Podman instalado (el script de instalación lo instalará automáticamente si no está presente).

### Instalar / Actualizar en el VPS
Puedes clonar este repositorio y ejecutar el script principal de instalación:
```bash
git clone https://github.com/dani626/NeoxAgent.git
cd NeoxAgent
sudo bash scripts/setup.sh
```
Otras opciones:
- `--update`: Actualiza el código a la última versión, recompila y reinicia el servicio manteniendo la configuración.
- `--reinstall`: Realiza una instalación limpia desde cero eliminando todo lo anterior.

### Pruebas de Funcionamiento

Dispones de scripts en la carpeta `scripts/` para verificar el estado de la instalación de forma local o externa:

#### 1. Verificación local de rutas y servicios (`verify_paths.sh`)
Se ejecuta directamente en el VPS para verificar que los ejecutables y archivos del servicio están en su lugar:
```bash
bash scripts/verify_paths.sh
```

#### 2. Prueba de ciclo de vida local (`test_lifecycle.sh`)
Se ejecuta en el VPS para verificar la creación, parada y eliminación de pods a través de la API local:
```bash
API_KEY="tu-clave-secreta" PORT=8443 bash scripts/test_lifecycle.sh
```

#### 3. Prueba de integración desde una máquina externa (`test_vps.sh`)
Este script se ejecuta desde tu máquina local (o entorno externo con `curl` y `jq` instalados) apuntando a la dirección IP pública del VPS de producción:
```bash
# Formato de uso:
# bash scripts/test_vps.sh -k <api_key> -h <vps_ip> [-p <port>] [-s] [-i]
#
# Ejemplo:
bash scripts/test_vps.sh -k "tu-clave-secreta" -h "192.168.1.100" -p 8443 -i
```
Este script realiza pruebas exhaustivas sobre:
* Endpoint de Salud (`/api/health`)
* Información y Telemetría del Sistema (`/api/system/info`, `/api/system/resources`)
* CRUD de Volúmenes (`/api/volumes`)
* CRUD de Redes (`/api/networks`)
* Creación, detención y eliminación de Pods reales (`/api/pods`)

