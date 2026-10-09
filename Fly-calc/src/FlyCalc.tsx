// Copyright (c) 2026 Fly-calc contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import React, { useEffect, useRef, useState } from 'react';
import { Aircraft, DocumentInfo, Probe, Status, request } from './client';
import './flycalc.css';

const SERVICE = 'http://127.0.0.1:8147';

export function FlyCalc() {
  const base = SERVICE;
  const fenix = typeof window !== 'undefined' && (window.location.pathname === '/fenix' || window.location.search.indexOf('display=fenix') >= 0);
  const embedded = typeof window !== 'undefined' && (window.location.protocol === 'coui:' || window.location.search.indexOf('display=flypad') >= 0);
  const [departure, setDeparture] = useState('UUEE');
  const [arrival, setArrival] = useState('ULLI');
  const [aircraft, setAircraft] = useState(fenix ? 'fenix-a320' : 'fbw-a32nx');
  const [fleet, setFleet] = useState<Aircraft[]>([]);
  const [status, setStatus] = useState<Status | null>(null);
  const [probe, setProbe] = useState<Probe | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [tab, setTab] = useState('flight');
  const [documents, setDocuments] = useState<DocumentInfo[]>([]);
  const [document, setDocument] = useState<DocumentInfo | null>(null);
  const [page, setPage] = useState(0);
  const [chartIcao, setChartIcao] = useState('UUEE');
  const alive = useRef(true);
  const [connectionTick, setConnectionTick] = useState(0);

  useEffect(() => {
    alive.current = true;
    let active = true;
    Promise.all([request<Status>(base, '/api/v1/status'), request<Aircraft[]>(base, '/api/v1/aircraft')])
      .then(([s, f]) => { if (active) { setStatus(s); setFleet(f); setError(''); } })
      .catch((e: Error) => { if (active) { setStatus(null); setError(e.message); } });
    return () => { active = false; alive.current = false; };
  }, [base, connectionTick]);

  const loadDocuments = () => request<DocumentInfo[]>(base, '/api/v1/documents')
    .then((items) => { if (alive.current) setDocuments(items); });

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setBusy(true); setError(''); setProbe(null);
    try {
      const result = await request<Probe>(base, '/api/v1/probes', 'POST', JSON.stringify({ departure, arrival, aircraft }));
      if (alive.current) { setProbe(result); setStatus(await request<Status>(base, '/api/v1/status')); }
    } catch (e) { if (alive.current) setError(e instanceof Error ? e.message : 'Ошибка запроса'); }
    finally { if (alive.current) setBusy(false); }
  };

  const chooseTab = (value: string) => {
    setTab(value); setError('');
    if (value === 'documents') loadDocuments().catch((e: Error) => { if (alive.current) setError(e.message); });
  };

  const upload = async (file: File | undefined) => {
    if (!file) return;
    setBusy(true); setError('');
    try {
      if (file.size > 20 * 1024 * 1024) throw new Error('PDF должен быть меньше 20 МБ');
      const bytes = await new Promise<ArrayBuffer>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result as ArrayBuffer);
        reader.onerror = () => reject(new Error('Не удалось прочитать PDF'));
        reader.readAsArrayBuffer(file);
      });
      const item = await request<DocumentInfo>(base,
        '/api/v1/documents?icao=' + encodeURIComponent(chartIcao) + '&label=' + encodeURIComponent(file.name), 'POST', bytes);
      if (alive.current) { setDocument(item); setPage(0); await loadDocuments(); }
    } catch (e) { if (alive.current) setError(e instanceof Error ? e.message : 'Не удалось добавить PDF'); }
    finally { if (alive.current) setBusy(false); }
  };

  return (
    <div className={'fc-root' + (fenix ? ' fc-fenix' : '') + (embedded ? ' fc-embedded' : '')}>
      <aside className="fc-sidebar">
        <div className="fc-logo"><span>F</span><strong>Fly-calc</strong></div>
        <p className="fc-eyebrow">FLIGHT DISPATCH</p>
        <button className={tab === 'flight' ? 'active' : ''} onClick={() => chooseTab('flight')}><span>↗</span> План полёта</button>
        <button className={tab === 'documents' ? 'active' : ''} onClick={() => chooseTab('documents')}><span>▤</span> Документы</button>
        <button className={tab === 'data' ? 'active' : ''} onClick={() => chooseTab('data')}><span>◎</span> База и самолёты</button>
        <div className="fc-sidebar-bottom"><span className={'fc-dot ' + (status ? 'online' : '')} /><span role="status">{status ? 'Локальный сервис подключён' : 'Нет соединения'}</span><small>Технический прототип · v0.1</small></div>
      </aside>
      <main className="fc-main">
        <header className="fc-header"><div><p className="fc-eyebrow">{fenix ? 'FENIX · ИНТЕРФЕЙС ДЛЯ ПЛАНШЕТА' : 'YOUR NEXT FLIGHT'}</p><h1>{tab === 'flight' ? 'Подготовка рейса' : tab === 'documents' ? 'Полётные документы' : 'Источники данных'}</h1></div><span className="fc-badge">RESEARCH PREVIEW</span></header>
        {fenix && <div className="fc-notice">Интерфейс адаптирован под экран Fenix. Добавление вкладки в штатный планшет самолёта пока не подтверждено.</div>}
        <div className="fc-notice"><strong>Расчёт пока недоступен.</strong> Проверяем интеграцию и данные. Бесплатная полная мировая база процедур и точность профилей ещё не подтверждены.</div>
        {error && <div className="fc-error" role="alert">{error}<button onClick={() => setConnectionTick(connectionTick + 1)}>Переподключить</button></div>}
        {tab === 'flight' && <>
          <form className="fc-card fc-form" onSubmit={submit}>
            <div className="fc-card-title"><h2>Куда летим?</h2><span>01 / РЕЙС</span></div>
            <div className="fc-route-inputs">
              <label>АЭРОПОРТ ВЫЛЕТА<input aria-label="ICAO вылета" value={departure} maxLength={4} pattern="[A-Za-z]{4}" required onChange={(e) => setDeparture(e.target.value.toUpperCase())} /><small>ICAO · 4 буквы</small></label>
              <button type="button" className="fc-swap" aria-label="Поменять аэропорты" onClick={() => { setDeparture(arrival); setArrival(departure); }}>⇄</button>
              <label>АЭРОПОРТ ПРИЛЁТА<input aria-label="ICAO прилёта" value={arrival} maxLength={4} pattern="[A-Za-z]{4}" required onChange={(e) => setArrival(e.target.value.toUpperCase())} /><small>ICAO · 4 буквы</small></label>
            </div>
            <label className="fc-aircraft-label">САМОЛЁТ<select value={aircraft} onChange={(e) => setAircraft(e.target.value)}>{fleet.length ? fleet.map((a) => <option value={a.id} key={a.id}>{a.name} · {a.simulator}</option>) : <option value="fbw-a32nx">FlyByWire A32NX</option>}</select></label>
            <div className="fc-submit-row"><p>Проверим аэропорты и связь с сервисом.<br />Маршрут и топливо не рассчитываются.</p><button className="fc-primary" disabled={busy || !status}>{busy ? 'Проверяем…' : 'Проверить рейс'} <span>→</span></button></div>
          </form>
          {probe && <section className="fc-card" aria-live="polite">
            <div className="fc-card-title"><h2>Связь проверена</h2><span>ТЕСТОВЫЙ ОТВЕТ</span></div>
            <div className="fc-airport-pair"><div><strong>{probe.departure.icao}</strong><p>{probe.departure.name}</p><small>{probe.departure.country}</small></div><span>→</span><div><strong>{probe.arrival.icao}</strong><p>{probe.arrival.name}</p><small>{probe.arrival.country}</small></div></div>
            <div className="fc-metrics">{['Маршрут', 'Запасной', 'Топливо', 'Время'].map((label) => <div key={label}><span>{label}</span><strong>—</strong><small>Не рассчитано</small></div>)}</div>
            {probe.document && <button className="fc-secondary" onClick={() => { setDocument(probe.document); setPage(0); chooseTab('documents'); }}>Открыть тестовый документ</button>}
          </section>}
          {!probe && <div className="fc-bottom-grid"><section className="fc-card"><p className="fc-eyebrow">НАВИГАЦИЯ</p><h2>{status ? status.airportCount.toLocaleString('ru-RU') : '—'} аэропортов</h2><p>OurAirports · мировой справочник<br />{status ? status.sources.enrouteCycle : 'Точки и трассы · нет связи'}</p></section><section className="fc-card"><p className="fc-eyebrow">ПРОФИЛИ САМОЛЁТОВ</p><h2>6 моделей</h2><p>MSFS и X-Plane<br />Калибровка до 5% ещё не выполнена</p></section></div>}
        </>}
        {tab === 'documents' && <div className="fc-doc-layout">
          <section className="fc-card fc-doc-list"><h2>OFP и чарты</h2><p>Выберите PDF из списка. Документы показаны как изображения страниц.</p>
            {documents.map((item) => <button key={item.id} className={'fc-doc-item ' + (document && item.id === document.id ? 'selected' : '')} onClick={() => { setDocument(item); setPage(0); }}><strong>{item.icao || 'TEST'} · {item.kind === 'probe' ? 'Тестовый OFP' : 'PDF'}</strong><small>{item.label}</small></button>)}
            {!documents.length && <p>Документов пока нет.</p>}
            <label>ICAO аэропорта<input aria-label="ICAO чарта" value={chartIcao} maxLength={4} onChange={(e) => setChartIcao(e.target.value.toUpperCase())} /></label>
            <label className="fc-upload">Добавить свой PDF<input aria-label="Добавить PDF" type="file" accept="application/pdf,.pdf" disabled={busy || !status || !status.documentsAvailable} onChange={(e) => { upload(e.target.files ? e.target.files[0] : undefined); e.target.value = ''; }} /></label>
            <small>Выбор локального файла доступен в Windows-интерфейсе. Возможность выбора файла в симуляторе требует отдельной проверки.</small>
          </section>
          <section className="fc-card fc-doc-view">{document ? <><div className="fc-page-tools"><button disabled={!page} onClick={() => setPage(page - 1)}>←</button><span>Страница {page + 1} / {document.pages}</span><button disabled={page + 1 >= document.pages} onClick={() => setPage(page + 1)}>→</button><a href={base + document.pdfUrl} target="_blank" rel="noreferrer">PDF ↗</a></div><img src={base + document.pageUrls[page]} alt={document.label + ', страница ' + (page + 1)} onError={() => setError('Не удалось загрузить страницу документа. Проверьте локальный сервис.')} /></> : <div className="fc-doc-empty">▤<h2>Документы вашего рейса</h2><p>Выберите файл, чтобы открыть его здесь.</p></div>}</section>
        </div>}
        {tab === 'data' && <section className="fc-card"><div className="fc-card-title"><h2>Готовность к полноценному расчёту</h2><span>НЕ ПОДТВЕРЖДЕНА</span></div><div className="fc-source-row"><strong>Аэропорты</strong><span>{status ? status.sources.airports : 'Нет связи'}</span></div><div className="fc-source-row"><strong>Маршрутные точки и трассы</strong><span>FlightGear · 2013.10 · исследовательская база</span></div><div className="fc-source-row"><strong>Мировые процедуры</strong><span>Полный бесплатный источник не подтверждён</span></div><div className="fc-source-row"><strong>RAD / NOTAM</strong><span>Не проверяются в прототипе</span></div><h2>Парк самолётов</h2>{fleet.map((a) => <div className="fc-source-row" key={a.id}><strong>{a.name}</strong><span>{a.simulator} · профиль не проверен</span></div>)}</section>}
        <footer className="fc-footer">FLY-CALC <span>Для авиасимуляторов · исследовательский этап</span></footer>
      </main>
    </div>
  );
}
