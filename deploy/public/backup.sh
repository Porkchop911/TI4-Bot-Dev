#!/usr/bin/env bash
# Tar the playtest data dir (games, player sessions, feedback) with retention.
# Run from root's or ti4's crontab, e.g. nightly:
#   17 3 * * *  /path/to/repo/deploy/public/backup.sh
#
# Ideally BACKUP_DIR is on another disk or a mounted Hetzner Storage Box / volume.
set -euo pipefail

DATA_DIR="${TI4_DATA_DIR:-/var/lib/ti4-public}"
BACKUP_DIR="${TI4_BACKUP_DIR:-/var/backups/ti4-public}"
KEEP_DAYS="${TI4_BACKUP_KEEP_DAYS:-14}"

mkdir -p "$BACKUP_DIR"
chmod 750 "$BACKUP_DIR"
out="$BACKUP_DIR/ti4-public-$(date -u +%Y%m%dT%H%M%SZ).tar.gz"
# Files are written atomically (tmp + rename), so a live tar is consistent enough; a game that
# changed while archiving replays from its log on recovery.
tar -C "$(dirname "$DATA_DIR")" -czf "$out.tmp" "$(basename "$DATA_DIR")"
mv "$out.tmp" "$out"
find "$BACKUP_DIR" -name 'ti4-public-*.tar.gz' -mtime "+$KEEP_DAYS" -delete
echo "backup written: $out ($(du -h "$out" | cut -f1))"
