import Link from "next/link";

import { DOCS } from "@/lib/docs";

export default function DocsIndexPage() {
  return (
    <main className="container">
      <h1>Otell Docs</h1>
      <div className="selector">
        {DOCS.map((doc) => (
          <Link key={doc.slug} className="docButton" href={`/docs/${doc.slug}`}>
            {doc.title}
          </Link>
        ))}
      </div>
    </main>
  );
}
