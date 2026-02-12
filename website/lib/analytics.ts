"use client";

import posthog from "posthog-js";

const POSTHOG_KEY = process.env["NEXT_PUBLIC_POSTHOG_KEY"] ?? "";
const POSTHOG_HOST = process.env["NEXT_PUBLIC_POSTHOG_HOST"] ?? "https://us.i.posthog.com";
let initialized = false;

export function initPosthog() {
  if (!POSTHOG_KEY || typeof window === "undefined" || initialized) {
    return;
  }

  posthog.init(POSTHOG_KEY, {
    api_host: POSTHOG_HOST,
    autocapture: true,
    capture_pageview: true,
    capture_pageleave: true,
  });

  initialized = true;
}
