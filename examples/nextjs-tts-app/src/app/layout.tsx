import type { Metadata } from 'next';
import { Inter } from 'next/font/google';
import './globals.css';

const inter = Inter({ subsets: ['latin'] });

export const metadata: Metadata = {
  title: 'Real-Time Text-to-Speech | RemoteMedia',
  description: 'Convert text to natural-sounding speech using Kokoro TTS and the RemoteMedia gRPC service',
  keywords: ['text-to-speech', 'TTS', 'Kokoro', 'RemoteMedia', 'speech synthesis'],
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body className={inter.className}>{children}</body>
    </html>
  );
}
