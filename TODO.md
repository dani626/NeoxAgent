# NeoxAgent — Pendientes

## 🔒 Seguridad — Filtrado de red (PENDIENTE)

El agente actualmente **no tiene filtrado de red a nivel de código**.
Solo depende de la API key como autenticación.

### Problemas identificados:
- [ ] **Sin IP whitelist** — acepta conexiones desde cualquier IP. Agregar middleware que solo permita peticiones desde la IP del panel (Jexactyl).
- [ ] **CORS completamente abierto** — `allow_origin(Any)`. Restringir al dominio del panel.
- [ ] **Sin rate limiting** — vulnerable a fuerza bruta de API key. Añadir `tower-governor` o similar.
- [ ] **Sin IP whitelist en código** — como mínimo leer `ALLOWED_IPS` desde `config.toml` y rechazar el resto con 403.

### Mitigación temporal recomendada (a nivel VPS):
```bash
# Solo permitir acceso al puerto del agente desde la IP del panel
ufw allow from <IP_DEL_PANEL> to any port <PUERTO_NEOXAGENT>
ufw deny <PUERTO_NEOXAGENT>
```

---

## Otros pendientes

- [ ] Limpiar warnings de compilación (`unused variables`, `dead_code`)
