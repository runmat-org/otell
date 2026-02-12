"use client";

import { useEffect } from "react";

import { initPosthog } from "@/lib/analytics";

export function AnalyticsBootstrap() {
  useEffect(() => {
    initPosthog();
  }, []);

  return null;
}
