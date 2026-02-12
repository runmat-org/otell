import fs from "node:fs/promises";

import type { Metadata } from "next";
import { notFound } from "next/navigation";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

import { DOCS, getDocBySlug } from "@/lib/docs";

type PageParams = {
  slug: string;
};

type PageProps = {
  params: Promise<PageParams>;
};

export function generateStaticParams(): PageParams[] {
  return DOCS.map((item) => ({ slug: item.slug }));
}

export async function generateMetadata({ params }: PageProps): Promise<Metadata> {
  const { slug } = await params;
  const doc = getDocBySlug(slug);

  if (!doc) {
    return { title: "Docs | otell" };
  }

  return {
    title: `${doc.title} | otell docs`,
    description: `${doc.title} documentation for otell.dev`,
  };
}

export default async function DocPage({ params }: PageProps) {
  const { slug } = await params;
  const doc = getDocBySlug(slug);

  if (!doc) {
    notFound();
  }

  const markdown = await fs.readFile(doc.sourcePath, "utf8");

  return (
    <main className="docContainer">
      <article className="docArticle">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>{markdown}</ReactMarkdown>
      </article>
    </main>
  );
}
