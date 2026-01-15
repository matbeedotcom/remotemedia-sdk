import React from 'react';
import ReactDOM from 'react-dom/client';
import { HashRouter, Routes, Route, Navigate } from 'react-router-dom';
import App from './App';
import { LandingRoute } from './pages/LandingRoute';
import { PERSONAS } from './config/personas';
import './index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <HashRouter>
      <Routes>
        {/* Landing pages - dynamically generated from PERSONAS config */}
        {PERSONAS.map((persona) => (
          <Route
            key={persona.id}
            path={persona.slug === 'general' ? '/' : `/${persona.slug}`}
            element={<LandingRoute slug={persona.slug} />}
          />
        ))}

        {/* Observer UI */}
        <Route path="/observe" element={<App />} />

        {/* Catch-all: redirect to root landing */}
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </HashRouter>
  </React.StrictMode>
);
