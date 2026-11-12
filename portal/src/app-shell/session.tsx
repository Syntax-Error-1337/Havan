import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { getToken, onTokenChange, setToken } from '../shared/api/client';

type SessionValue = {
  token: string | null;
  signIn: (token: string) => void;
  signOut: () => void;
};

const SessionContext = createContext<SessionValue>({
  token: null,
  signIn: () => {},
  signOut: () => {},
});

export function SessionProvider({ children }: { children: ReactNode }) {
  const [token, setTokenState] = useState<string | null>(getToken());

  // The API client clears the token on a 401. Follow it: otherwise the gate keeps
  // rendering the signed-in shell and the page shows "missing bearer token".
  useEffect(() => onTokenChange(() => setTokenState(getToken())), []);

  const signIn = useCallback((next: string) => {
    setToken(next);
    setTokenState(next);
  }, []);

  const signOut = useCallback(() => {
    setToken(null);
    setTokenState(null);
  }, []);

  const value = useMemo(() => ({ token, signIn, signOut }), [token, signIn, signOut]);
  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

export function useSession() {
  return useContext(SessionContext);
}
