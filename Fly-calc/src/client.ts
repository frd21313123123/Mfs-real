export interface Airport {
  icao: string;
  name: string;
  country: string;
  latitude: number;
  longitude: number;
}
export interface Aircraft {
  id: string;
  name: string;
  simulator: string;
  status: 'unverified';
  accuracyPercent: null;
}
export interface Probe {
  id: string;
  status: 'integration-probe';
  departure: Airport;
  arrival: Airport;
  aircraft: Aircraft;
  route: null;
  alternate: null;
  fuel: null;
  duration: null;
  mass: null;
  warnings: string[];
  document: DocumentInfo | null;
}
export interface DocumentInfo {
  id: string;
  label: string;
  icao: string;
  kind: string;
  pages: number;
  pdfUrl: string;
  pageUrls: string[];
}
export interface Status {
  apiVersion: '1';
  worldRoutingReady: false;
  airportCount: number;
  documentsAvailable: boolean;
  sources: { airports: string; enrouteCycle: string; procedures: string };
  fenix: { cockpitIntegration: 'unconfirmed'; companionUrl: string };
}

// Use the existing flyPad XHR style and ES2017; actual simulator runtime still needs validation.
// Do not introduce Node APIs or depend on AbortController in the instrument.
export function request<T>(base: string, path: string, method = 'GET', data?: string | ArrayBuffer): Promise<T> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open(method, base + path);
    xhr.timeout = method === 'POST' ? 55000 : 15000;
    if (typeof data === 'string') xhr.setRequestHeader('Content-Type', 'application/json');
    else if (data) xhr.setRequestHeader('Content-Type', 'application/pdf');
    xhr.onload = () => {
      try {
        const result = JSON.parse(xhr.responseText);
        if (xhr.status < 200 || xhr.status >= 300) reject(new Error(result.error || 'Ошибка сервиса'));
        else resolve(result as T);
      } catch (_) { reject(new Error('Сервис вернул некорректный ответ')); }
    };
    xhr.onerror = () => reject(new Error('Нет связи с Fly-calc. Запустите локальное приложение Windows.'));
    xhr.ontimeout = () => reject(new Error('Сервис не ответил за ' + xhr.timeout / 1000 + ' секунд. Повторите запрос.'));
    xhr.send(data === undefined ? null : data);
  });
}
