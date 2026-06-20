import type { Metadata } from "next";
import { Cinzel, Oswald, Spectral, Bangers } from "next/font/google";
import "./globals.css";

const cinzel = Cinzel({ subsets: ["latin"], weight: ["600", "700", "800"], variable: "--font-cinzel" });
const oswald = Oswald({ subsets: ["latin"], weight: ["400", "500", "600", "700"], variable: "--font-oswald" });
const spectral = Spectral({
  subsets: ["latin"],
  weight: ["400", "600", "700"],
  style: ["normal", "italic"],
  variable: "--font-spectral",
});
const bangers = Bangers({ subsets: ["latin"], weight: ["400"], variable: "--font-bangers" });

export const metadata: Metadata = {
  title: "TCGOP — One Piece Grand Line TCG",
  description: "Un jeu de cartes à collectionner dans l'univers de One Piece",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="fr" className={`${cinzel.variable} ${oswald.variable} ${spectral.variable} ${bangers.variable}`}>
      <body className="min-h-screen antialiased text-white" style={{ background: "#06080c", fontFamily: "var(--font-spectral), Georgia, serif" }}>
        {children}
      </body>
    </html>
  );
}
