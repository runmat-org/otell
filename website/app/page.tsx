"use client";

import { useMemo, useState } from "react";

type Platform = "macos" | "linux" | "windows";

function detectPlatform(): Platform {
  if (typeof navigator === "undefined") {
    return "macos";
  }
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes("win")) {
    return "windows";
  }
  if (ua.includes("linux")) {
    return "linux";
  }
  return "macos";
}

export default function Home() {
  const [platform, setPlatform] = useState<Platform>(() => detectPlatform());
  const [copied, setCopied] = useState<boolean>(false);

  const installCommand = useMemo((): string => {
    if (platform === "windows") {
      return "iwr https://otell.dev/install.ps1 -useb | iex";
    }
    return "curl -fsSL https://otell.dev/install.sh | sh";
  }, [platform]);

  const onCopy = async (): Promise<void> => {
    try {
      await navigator.clipboard.writeText(installCommand);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      setCopied(false);
    }
  };

  return (
    <main className="container">

      <section className="installPanel" aria-label="Installer">
        <div className="selector" role="tablist" aria-label="Operating system">
          <button
            type="button"
            role="tab"
            aria-selected={platform === "macos" || platform === "linux"}
            className={platform === "macos" || platform === "linux" ? "active" : ""}
            onClick={() => setPlatform("linux")}
          >
            Linux / macOS
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={platform === "windows"}
            className={platform === "windows" ? "active" : ""}
            onClick={() => setPlatform("windows")}
          >
            Windows
          </button>
        </div>

        <div className="commandRow">
          <pre>{installCommand}</pre>
          <button type="button" onClick={onCopy} className="copyButton">
            {copied ? "Copied" : "Copy"}
          </button>
        </div>
      </section>

      <p>
      After install, run <code>otell intro</code> to verify setup and begin.
      </p>
    </main>
  );
}
