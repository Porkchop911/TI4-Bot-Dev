import { defineConfig, mergeConfig } from "vite";
import base from "../../../vite.config";

// The app's vite config without the file watcher: captures never edit sources, and the watcher
// fails with ENOSPC on machines with a low inotify limit.
export default mergeConfig(base, defineConfig({ server: { watch: null } }));
