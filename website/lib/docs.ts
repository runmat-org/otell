import path from "node:path";

export type DocItem = {
  slug: string;
  title: string;
  sourcePath: string;
};

const repoRoot = path.resolve(process.cwd(), "..");

export const DOCS: readonly DocItem[] = [
  {
    slug: "api",
    title: "API",
    sourcePath: path.join(repoRoot, "docs", "API.md"),
  },
  {
    slug: "cli",
    title: "CLI",
    sourcePath: path.join(repoRoot, "docs", "CLI.md"),
  },
  {
    slug: "config",
    title: "Config",
    sourcePath: path.join(repoRoot, "docs", "CONFIG.md"),
  },
];

export function getDocBySlug(slug: string): DocItem | undefined {
  return DOCS.find((item) => item.slug === slug);
}
