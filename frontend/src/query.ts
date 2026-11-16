import { useQuery } from "@tanstack/react-query";
import { fetchWorkspace } from "./api";

export const workspaceQueryKey = ["workspace"] as const;

export function useWorkspace() {
  return useQuery({
    queryKey: workspaceQueryKey,
    queryFn: fetchWorkspace,
    refetchOnWindowFocus: false,
  });
}
