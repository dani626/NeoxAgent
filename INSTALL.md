# neoxagent Installation Guide

This guide details the requirements and steps to deploy neoxagent on a fresh Linux VPS.

## 📋 Requirements
- **OS**: Debian 11/12 (Recommended) or Ubuntu 20.04/22.04.
- **Architecture**: x86_64 (amd64).
- **Permissions**: Root access (sudo is required).
- **Resources**: Min 1GB RAM, 1 CPU Core.

## 🔓 Network Ports
Ensure the following ports are open in your **Cloud Provider Firewall**:
- **80 (TCP)**: HTTP Traffic (Nginx/Web Servers).
- **443 (TCP)**: HTTPS Traffic.
- **8443 (TCP)**: neoxagent API (Internal or External access).
- **SSH Port**: Default 22 (or your custom port, e.g., 51821).

## 🚀 Installation Steps

### 1. Prepare Files
You need the compiled binary and configuration file on your local machine:
- `target/release/neoxagent`
- `config.toml`
- `install_neox.sh` (The provided script)

### 2. Upload to VPS
Use `scp` to upload files to your server (replace variables accordingly):

```bash
# Example
scp -P 51821 target/release/neoxagent config.toml install_neox.sh root@<VPS_IP>:/root/
```

### 3. Run the Installer
SSH into your VPS and execute the script:

```bash
ssh -p 51821 root@<VPS_IP>
chmod +x install_neox.sh
./install_neox.sh
```

## ✅ Verification
After installation, verify the service status:

```bash
systemctl status neoxagent
curl http://127.0.0.1:8443/api/health
```

## ⚠️ Troubleshooting
- **"Config not found"**: The installer creates a symlink `/root/config.toml` -> `/etc/neoxagent.toml`. Ensure you are running the agent from the correct WorkingDirectory or rely on the service.
- **Connection Refused**: Check external provider firewalls (AWS Security Groups, DigitalOcean Firewalls, etc.).
