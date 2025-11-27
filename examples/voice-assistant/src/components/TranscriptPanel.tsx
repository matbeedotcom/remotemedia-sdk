import { useEffect, useRef } from 'react';
import { User, Bot } from 'lucide-react';

interface Message {
  id: string;
  type: 'user' | 'assistant';
  text: string;
  timestamp: number;
  isPartial?: boolean;
}

interface TranscriptPanelProps {
  messages: Message[];
  className?: string;
}

export function TranscriptPanel({ messages, className }: TranscriptPanelProps) {
  const bottomRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  if (messages.length === 0) {
    return (
      <div className={`flex items-center justify-center ${className}`}>
        <p className="text-gray-500">
          Click the microphone or type a message to start
        </p>
      </div>
    );
  }

  return (
    <div className={`p-4 space-y-4 ${className}`}>
      {messages.map((message) => (
        <div
          key={message.id}
          className={`flex gap-3 ${
            message.type === 'user' ? 'justify-end' : 'justify-start'
          }`}
        >
          {message.type === 'assistant' && (
            <div className="w-8 h-8 rounded-full bg-purple-600 flex items-center justify-center flex-shrink-0">
              <Bot className="w-5 h-5 text-white" />
            </div>
          )}

          <div
            className={`
              max-w-[70%] px-4 py-2 rounded-lg
              ${message.type === 'user'
                ? 'bg-blue-600 text-white rounded-br-sm'
                : 'bg-gray-700 text-white rounded-bl-sm'
              }
              ${message.isPartial ? 'opacity-70' : ''}
            `}
          >
            <p className="whitespace-pre-wrap">{message.text}</p>
            <span className="text-xs opacity-60 mt-1 block">
              {new Date(message.timestamp).toLocaleTimeString()}
              {message.isPartial && ' (typing...)'}
            </span>
          </div>

          {message.type === 'user' && (
            <div className="w-8 h-8 rounded-full bg-blue-600 flex items-center justify-center flex-shrink-0">
              <User className="w-5 h-5 text-white" />
            </div>
          )}
        </div>
      ))}
      <div ref={bottomRef} />
    </div>
  );
}
