"use client";

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Suspense, useState } from "react";
import { ToastProvider } from "@/components/ui/ToastContext";
import { Toaster } from "@/components/ui/Toaster";
import { AnalyticsProvider } from "@/components/analytics/AnalyticsProvider";

export function Providers({ children }: { children: React.ReactNode }) {
  const [client] = useState(() => new QueryClient());
  return (
    <QueryClientProvider client={client}>
      <ToastProvider>
        <Suspense>
          <AnalyticsProvider>
            {children}
            <Toaster />
          </AnalyticsProvider>
        </Suspense>
      </ToastProvider>
    </QueryClientProvider>
  );
}
