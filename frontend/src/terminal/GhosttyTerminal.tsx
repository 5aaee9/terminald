import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { FitAddon, Terminal, init } from "ghostty-web";

export interface TerminalHandle {
  write(data: Uint8Array): void;
}

interface Props {
  onData(data: string): void;
  onResize(cols: number, rows: number): void;
}

const GhosttyTerminal = forwardRef<TerminalHandle, Props>(function GhosttyTerminal(
  { onData, onResize },
  ref
) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);

  useImperativeHandle(ref, () => ({
    write(data: Uint8Array) {
      terminalRef.current?.write(data);
    },
  }));

  useEffect(() => {
    let disposed = false;
    let resizeObserver: ResizeObserver | undefined;

    async function mount() {
      await init();
      if (disposed || !containerRef.current) {
        return;
      }

      const terminal = new Terminal({
        cursorBlink: false,
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
        fontSize: 14,
        theme: {
          background: "#101418",
          foreground: "#d8e0e7",
        },
      });
      const fit = new FitAddon();
      terminal.loadAddon(fit);
      terminal.open(containerRef.current);
      terminal.onData(onData);
      terminalRef.current = terminal;

      const fitAndNotify = () => {
        fit.fit();
        onResize(terminal.cols, terminal.rows);
      };
      fitAndNotify();
      resizeObserver = new ResizeObserver(fitAndNotify);
      resizeObserver.observe(containerRef.current);
      terminal.focus();
    }

    mount();
    return () => {
      disposed = true;
      resizeObserver?.disconnect();
      terminalRef.current?.dispose();
      terminalRef.current = null;
    };
  }, [onData, onResize]);

  return <div className="terminal" ref={containerRef} />;
});

export default GhosttyTerminal;
