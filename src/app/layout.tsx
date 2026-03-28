import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "One Piece Grand Line TCG",
  description: "Un jeu de cartes a collectionner dans l'univers de One Piece",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="fr">
      <body className="bg-gray-950 text-white min-h-screen antialiased">
        {children}
      </body>
    </html>
  );
}
