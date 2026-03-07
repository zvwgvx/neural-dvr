import type { Metadata } from 'next';
import './globals.css';

export const metadata: Metadata = {
  title: 'Neural DVR — Hikvision Live Dashboard',
  description: 'Realtime video streaming dashboard for Hikvision DVR systems. Monitor cameras, manage streams, and view live feeds.',
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <head>
        <meta name="theme-color" content="#0a0e17" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
      </head>
      <body>
        <div className="app-container">
          {children}
        </div>
      </body>
    </html>
  );
}
