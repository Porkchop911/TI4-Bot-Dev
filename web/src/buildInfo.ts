import { useEffect, useState } from "react";

/** What `GET /health` says about the running server. */
export interface BuildInfo {
  commit: string;
  protocolVersion: number | null;
  /** True on the public playtest server (`TI4_PUBLIC=1`): Alpha notice and Feedback button show. */
  isPublic: boolean;
}

const NONE: BuildInfo = { commit: "dev", protocolVersion: null, isPublic: false };

let cached: Promise<BuildInfo> | null = null;

export function fetchBuildInfo(): Promise<BuildInfo> {
  cached ??= (async () => {
    try {
      const response = await fetch("/health");
      if (!response.ok) return NONE;
      const body = (await response.json()) as {
        commit?: string;
        protocol_version?: number;
        public?: boolean;
      };
      return {
        commit: typeof body.commit === "string" ? body.commit : "dev",
        protocolVersion: typeof body.protocol_version === "number" ? body.protocol_version : null,
        isPublic: body.public === true,
      };
    } catch {
      return NONE;
    }
  })();
  return cached;
}

export function resetBuildInfoForTests(): void {
  cached = null;
}

export function useBuildInfo(): BuildInfo {
  const [info, setInfo] = useState<BuildInfo>(NONE);
  useEffect(() => {
    let live = true;
    void fetchBuildInfo().then((value) => live && setInfo(value));
    return () => {
      live = false;
    };
  }, []);
  return info;
}
