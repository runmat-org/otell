import Link from "next/link";

import { DOCS } from "@/lib/docs";

export function SiteHeader() {
  return (
    <header className="siteHeader">
      <div className="siteHeaderInner">
        <Link href="/" className="brandLink">
          Otell
        </Link>
        <nav className="headerNav" aria-label="Primary">
          {DOCS.map((doc) => (
            <Link key={doc.slug} className="navButton" href={`/docs/${doc.slug}`}>
              {doc.title}
            </Link>
          ))}
          <a href="https://github.com/runmat-org/otell" className="navButton" target="_blank" rel="noopener noreferrer">GitHub</a>
        </nav>
      </div>
    </header>
  );
}
