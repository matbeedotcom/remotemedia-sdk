import type { Metadata } from 'next';
import { Inter } from 'next/font/google';
import Link from 'next/link';
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
      <body className={inter.className}>
        {/* Navigation */}
        <nav className="bg-white border-b border-gray-200 px-4 py-3">
          <div className="max-w-7xl mx-auto flex items-center gap-6">
            <Link href="/" className="font-bold text-gray-900 hover:text-indigo-600">
              RemoteMedia
            </Link>
            <div className="flex gap-4 text-sm">
              <Link href="/" className="text-gray-600 hover:text-indigo-600">
                TTS Demo
              </Link>
              <Link href="/webrtc-tts" className="text-gray-600 hover:text-indigo-600">
                WebRTC TTS
              </Link>
              <Link href="/webrtc-calculator" className="text-gray-600 hover:text-indigo-600">
                WebRTC Calculator
              </Link>
              <Link href="/s2s" className="text-gray-600 hover:text-indigo-600">
                Speech-to-Speech
              </Link>
            </div>
          </div>
        </nav>
        {children}
      </body>
    </html>
  );
}
