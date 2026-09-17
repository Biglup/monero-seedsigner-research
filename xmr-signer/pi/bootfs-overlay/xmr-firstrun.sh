#!/bin/sh
# One-shot setup for the benchmark card, run by systemd from the kernel
# command line (systemd.run=...) with the root filesystem writable and no
# services up. It runs the gate, installs SSH + USB gadget networking without
# relying on cloud-init, records diagnostics on the FAT partition, removes
# itself from cmdline.txt and reboots into a normal boot.
set -u
B=/boot/firmware
OUT=$B/xmr-gate-result.txt
LOG=$B/xmr-firstrun.log
exec >$LOG 2>&1
set -x
date -u

# 1. Day-one gate: 20 passes over the 256 upstream key image vectors.
cp $B/xmr-gate-armv6 /tmp/xmr-gate-armv6 && chmod +x /tmp/xmr-gate-armv6
{
  echo "=== xmr-gate on-device run $(date -u +%Y-%m-%dT%H:%M:%SZ) ==="
  uname -a
  grep -E "model name|Hardware|Revision|Model" /proc/cpuinfo
  grep MemTotal /proc/meminfo
  echo "---"
  /tmp/xmr-gate-armv6 20
  echo "exit=$?"
} >$OUT 2>&1

# 2. User pi with the development machine's key.
if ! id pi >/dev/null 2>&1; then
  useradd -m -s /bin/bash -G adm,sudo,video,plugdev,netdev pi
  echo 'pi:raspberry' | chpasswd
fi
echo 'pi ALL=(ALL) NOPASSWD:ALL' >/etc/sudoers.d/010_pi-nopasswd
chmod 440 /etc/sudoers.d/010_pi-nopasswd
install -d -m 700 -o pi -g pi /home/pi/.ssh
echo 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIADWiZWPIHd3x1EXEhO3CFwYJGm92pxJsUcjlpDlGXdk angel.castillob@protonmail.com' >/home/pi/.ssh/authorized_keys
chown pi:pi /home/pi/.ssh/authorized_keys; chmod 600 /home/pi/.ssh/authorized_keys

# 3. SSH.
systemctl enable ssh || true
systemctl enable regenerate_ssh_host_keys 2>/dev/null || true

# 4. USB gadget link: managed by NetworkManager, shared mode (Pi is 10.42.0.1 and serves DHCP).
install -d /etc/NetworkManager/conf.d /etc/NetworkManager/system-connections /etc/udev/rules.d
cat >/etc/NetworkManager/conf.d/10-usb0-managed.conf <<'EOT'
[device-usb0]
match-device=interface-name:usb0
managed=1
EOT
cat >/etc/udev/rules.d/86-usb0-nm-managed.rules <<'EOT'
ENV{DEVTYPE}=="gadget", ENV{NM_UNMANAGED}="0"
EOT
cat >/etc/NetworkManager/system-connections/usb0.nmconnection <<'EOT'
[connection]
id=usb0
type=ethernet
interface-name=usb0
autoconnect=true
autoconnect-priority=100

[ipv4]
method=shared

[ipv6]
method=link-local
EOT
chmod 600 /etc/NetworkManager/system-connections/usb0.nmconnection
systemctl enable NetworkManager avahi-daemon 2>/dev/null || true

# 5. Every-boot diagnostics + fallback that forces usb0 up.
cat >/usr/local/sbin/xmr-bench-autorun.sh <<'EOT'
#!/bin/sh
B=/boot/firmware
for i in 1 2 3 4 5 6; do [ -e /sys/class/net/usb0 ] && break; sleep 5; done
nmcli device set usb0 managed yes 2>/dev/null || true
nmcli connection up usb0 2>/dev/null || true
sleep 5
{
  echo "=== boot diagnostics $(date -u +%Y-%m-%dT%H:%M:%SZ) ==="
  echo "udc: $(ls /sys/class/udc 2>/dev/null)"
  lsmod | grep -E "dwc2|g_ether|usb_f|libcomposite" || echo "no gadget modules loaded"
  nmcli -t device 2>/dev/null; nmcli -t connection 2>/dev/null
  ip -br addr 2>/dev/null
  systemctl is-active ssh NetworkManager avahi-daemon 2>/dev/null
} >$B/xmr-boot-diag.txt 2>&1
sync
EOT
chmod 755 /usr/local/sbin/xmr-bench-autorun.sh
cat >/etc/systemd/system/xmr-bench-autorun.service <<'EOT'
[Unit]
Description=Benchmark card boot diagnostics and usb0 fallback
After=network-online.target NetworkManager.service
Wants=network-online.target

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/xmr-bench-autorun.sh

[Install]
WantedBy=multi-user.target
EOT
systemctl enable xmr-bench-autorun.service || true

# 6. Why did cloud-init not re-run? Keep the evidence.
{
  echo "--- cloud-init ---"
  ls -la /etc/cloud/cloud-init.disabled 2>&1 || true
  ls /var/lib/cloud/instances 2>&1 || true
  tail -20 /var/log/cloud-init.log 2>&1 || true
  echo "--- NM ---"
  ls -la /etc/NetworkManager/system-connections 2>&1 || true
  grep -rn gadget /usr/lib/udev/rules.d/85-nm-unmanaged.rules 2>&1 || true
  echo "--- users ---"; id pi
} >$B/xmr-firstrun-diag.txt 2>&1

# 7. Remove this hook from the kernel command line and reboot normally.
sed -i 's| systemd.run=[^ ]*||; s| systemd.run_success_action=[^ ]*||; s| systemd.unit=[^ ]*||' $B/cmdline.txt
sync
date -u
exit 0
