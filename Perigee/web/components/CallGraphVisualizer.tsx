'use client';

import React, { useEffect, useRef } from 'react';
import mermaid from 'mermaid';
import { useReducedMotion } from 'framer-motion';
import { logger } from '../lib/logger';

interface CallGraphVisualizerProps {
  mermaidDefinition: string;
}

export function CallGraphVisualizer({ mermaidDefinition }: CallGraphVisualizerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const reducedMotion = useReducedMotion();

  useEffect(() => {
    mermaid.initialize({
      startOnLoad: false,
      theme: 'dark',
      securityLevel: 'loose',
      flowchart: {
        useMaxWidth: true,
        htmlLabels: true,
        curve: 'basis',
      },
      ...(reducedMotion
        ? {
            themeCSS: `** { animation: none !important; transition: none !important; }`,
          }
        : {}),
    });
  }, [reducedMotion]);

  useEffect(() => {
    const renderMermaid = async () => {
      if (containerRef.current && mermaidDefinition) {
        try {
          containerRef.current.innerHTML = '';
          const { svg } = await mermaid.render('mermaid-graph-' + Date.now(), mermaidDefinition);
          containerRef.current.innerHTML = svg;
        } catch (error) {
          logger.error('Mermaid rendering failed:', error);
          containerRef.current.innerHTML = `<p style="color: #fb8500;">Failed to render call graph: ${error}</p>`;
        }
      }
    };

    renderMermaid();
  }, [mermaidDefinition]);

  return (
    <div style={{ marginTop: '20px' }}>
      <h4 style={{ color: '#00d9ff', fontSize: '14px', marginBottom: '12px', fontWeight: '600' }}>
        Cross-Contract Dependency Graph
      </h4>
      <div
        ref={containerRef}
        style={{
          backgroundColor: '#010409',
          padding: '16px',
          borderRadius: '8px',
          border: '1px solid #30363d',
          overflow: 'auto',
          minHeight: '100px',
        }}
      />
    </div>
  );
}