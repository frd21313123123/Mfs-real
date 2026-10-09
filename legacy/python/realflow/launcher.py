"""Standalone settings launcher; no simulator is started on opening."""
from __future__ import annotations
import argparse
from dataclasses import asdict
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading
import tkinter as tk
from tkinter import filedialog, messagebox, ttk
from .config import Settings


def save_profile(settings: Settings, path: Path):
    """Replace a profile only after validation and a complete write."""
    settings.validate()
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode='w', encoding='utf-8', dir=path.parent,
                                         delete=False, suffix='.json') as stream:
            temporary = Path(stream.name)
            json.dump(asdict(settings), stream, ensure_ascii=False, indent=2)
        os.replace(temporary, path)
    finally:
        if temporary and temporary.exists():
            temporary.unlink()


def launch_command(settings: Settings, profile: Path, action: str, output: Path,
                   duration: int = 60) -> list[str]:
    settings.validate()
    if getattr(sys, 'frozen', False):
        executable = Path(sys.executable).parent / 'RealFlowTraffic-ATC.exe'
        if not executable.is_file():
            raise ValueError('Поместите RealFlowTraffic-ATC.exe рядом с лаунчером.')
        command = [str(executable)]
    else:
        command = [sys.executable, '-m', 'realflow']
    command += ['--config', str(profile.resolve())]
    if action == 'demo':
        return command + ['demo', '--steps', '600', '--output', str(output.resolve())]
    if action != 'simconnect':
        raise ValueError('Неизвестный режим запуска')
    if os.name != 'nt':
        raise ValueError('MSFS / SimConnect доступен только в Windows.')
    if duration < 1:
        raise ValueError('Продолжительность должна быть больше нуля.')
    if not settings.fsltl_path or not Path(settings.fsltl_path).is_dir():
        raise ValueError('Выберите установленную папку FSLTL.')
    command += ['run', '--bridge', 'simconnect', '--fsltl', settings.fsltl_path,
                '--duration', str(duration)]
    if settings.airport_graph:
        if not Path(settings.airport_graph).is_file():
            raise ValueError('Файл графа аэропорта не найден.')
        if not settings.enable_simconnect_position_writes:
            raise ValueError('Движение на земле требует включения записи позиций.')
        command += ['--airport', settings.airport_graph]
    if settings.enable_simconnect_position_writes:
        command.append('--allow-motion')
    return command


class Launcher(tk.Tk):
    def __init__(self, profile: Path):
        super().__init__()
        self.title('RealFlow — лаунчер настроек')
        self.geometry('780x760')
        self.minsize(700, 680)
        self.profile = profile.resolve()
        self.process = None
        self.messages = queue.Queue()
        self.variables = {}
        self.status = tk.StringVar()
        toolbar = ttk.Frame(self, padding=12)
        toolbar.pack(fill='x')
        for title, callback in [('Открыть профиль', self.open_profile),
                                ('Сохранить', self.save), ('Сохранить как…', self.save_as)]:
            ttk.Button(toolbar, text=title, command=callback).pack(side='left', padx=3)
        ttk.Label(self, textvariable=self.status, wraplength=740).pack(fill='x', padx=16)
        tabs = ttk.Notebook(self)
        tabs.pack(fill='x', padx=12, pady=10)
        general = ttk.Frame(tabs, padding=12)
        advanced = ttk.Frame(tabs, padding=12)
        tabs.add(general, text='Трафик и пути')
        tabs.add(advanced, text='Дополнительно')
        labels = {
            'mode': 'Источник трафика', 'airborne_limit': 'Самолёты в воздухе (0–35)',
            'ground_limit': 'Самолёты на земле (0–30)', 'sync_real_flights': 'Получать данные OpenSky',
            'fetch_seconds': 'Интервал OpenSky, сек. (≥60)',
            'max_radius_km': 'Радиус трафика, км (2–250)', 'fsltl_path': 'Папка FSLTL',
            'airport_graph': 'Проверенный граф аэропорта JSON',
            'realistic_taxi': 'Реалистичное руление', 'runway_control': 'Контроль полосы',
            'fps_target': 'Целевой FPS',
            'enable_simconnect_position_writes': 'Экспериментальная запись позиций в MSFS',
        }
        defaults = asdict(Settings())
        for key in labels:
            parent = advanced if key in ('realistic_taxi', 'runway_control', 'fps_target',
                                         'enable_simconnect_position_writes') else general
            row = ttk.Frame(parent)
            row.pack(fill='x', pady=4)
            value = defaults[key]
            variable = tk.BooleanVar(value=value) if isinstance(value, bool) else tk.StringVar(value=str(value))
            self.variables[key] = variable
            if isinstance(value, bool):
                ttk.Checkbutton(row, text=labels[key], variable=variable).pack(anchor='w')
            else:
                ttk.Label(row, text=labels[key], width=38).pack(side='left')
                if key == 'mode':
                    ttk.Combobox(row, textvariable=variable, values=['hybrid', 'live', 'simulation'],
                                 state='readonly').pack(side='left', fill='x', expand=True)
                else:
                    ttk.Entry(row, textvariable=variable).pack(side='left', fill='x', expand=True)
                if key in ('fsltl_path', 'airport_graph'):
                    ttk.Button(row, text='…', width=3, command=lambda k=key: self.browse(k)).pack(side='left')
        ttk.Label(advanced, text='FPS, реалистичное руление и контроль полосы сохраняются в профиль,\n'
                  'но движок пока не применяет эти переключатели.\n'
                  'Запись позиций применяется при запуске MSFS через этот лаунчер.',
                  wraplength=680).pack(anchor='w', pady=10)
        actions = ttk.Frame(self, padding=12)
        actions.pack(fill='x')
        self.buttons = []
        for label, action in [('Запустить офлайн-демо', 'demo'), ('Запустить MSFS', 'simconnect')]:
            button = ttk.Button(actions, text=label, command=lambda a=action: self.launch(a))
            button.pack(side='left', padx=3)
            self.buttons.append(button)
        ttk.Label(actions, text='MSFS: длительность, сек.').pack(side='left', padx=6)
        self.duration = tk.StringVar(value='60')
        ttk.Entry(actions, textvariable=self.duration, width=7).pack(side='left')
        ttk.Label(self, text='Офлайн-демо не подключается к игре. MSFS должен быть уже запущен.\n'
                  'Изменения настроек действуют при следующем запуске.', wraplength=740).pack(anchor='w', padx=16)
        self.log = tk.Text(self, height=10, state='disabled', wrap='word')
        self.log.pack(fill='both', expand=True, padx=12, pady=10)
        try:
            self.load(self.profile)
        except (ValueError, OSError, TypeError) as exc:
            self.status.set(str(self.profile))
            messagebox.showerror('Не удалось загрузить профиль', str(exc), parent=self)
        self.protocol('WM_DELETE_WINDOW', self.close)
        self.after(100, self.poll)

    def load(self, path):
        settings = Settings.load(path)
        self.profile = Path(path).resolve()
        for key, value in asdict(settings).items():
            self.variables[key].set(value)
        self.status.set(str(self.profile))

    def collect(self):
        values = {}
        for key, default in asdict(Settings()).items():
            value = self.variables[key].get()
            values[key] = type(default)(value)
        if values['fps_target'] < 1:
            raise ValueError('Целевой FPS должен быть больше нуля.')
        return Settings(**values).validate()

    def browse(self, key):
        path = (filedialog.askdirectory(parent=self) if key == 'fsltl_path' else
                filedialog.askopenfilename(parent=self, filetypes=[('JSON', '*.json')]))
        if path:
            self.variables[key].set(path)

    def open_profile(self):
        path = filedialog.askopenfilename(parent=self, filetypes=[('JSON', '*.json')])
        if path:
            try:
                self.load(path)
            except (ValueError, OSError, TypeError) as exc:
                messagebox.showerror('Ошибка профиля', str(exc), parent=self)

    def save(self):
        try:
            save_profile(self.collect(), self.profile)
            self.status.set(f'Сохранено: {self.profile}')
            return True
        except (ValueError, OSError, tk.TclError) as exc:
            messagebox.showerror('Ошибка настроек', str(exc), parent=self)
            return False

    def save_as(self):
        path = filedialog.asksaveasfilename(parent=self, defaultextension='.json',
                                          filetypes=[('JSON', '*.json')])
        if path:
            previous = self.profile
            self.profile = Path(path).resolve()
            if not self.save():
                self.profile = previous

    def launch(self, action):
        if self.process is not None:
            return
        try:
            settings = self.collect()
            output = self.profile.parent / 'launcher-demo-results.json'
            command = launch_command(settings, self.profile, action, output,
                                     int(self.duration.get()) if action == 'simconnect' else 60)
            if action == 'simconnect' and not messagebox.askokcancel(
                    'Экспериментальный MSFS', 'Будет выполнено подключение к MSFS. '
                    'Нативная интеграция пока не проверена в игре. Продолжить?', parent=self):
                return
            if not self.save():
                return
            self.process = subprocess.Popen(command, cwd=self.profile.parent, stdout=subprocess.PIPE,
                                            stderr=subprocess.STDOUT, text=True, encoding='utf-8',
                                            errors='replace', env={**os.environ, 'PYTHONIOENCODING': 'utf-8',
                                                                    'PYTHONUNBUFFERED': '1'})
            for button in self.buttons:
                button.configure(state='disabled')
            self.status.set('Выполняется…')
            threading.Thread(target=self.read_output, args=(self.process,), daemon=True).start()
        except (ValueError, OSError, tk.TclError) as exc:
            messagebox.showerror('Ошибка запуска', str(exc), parent=self)

    def read_output(self, process):
        for line in process.stdout:
            self.messages.put(line)
        process.stdout.close()
        self.messages.put(process.wait())

    def poll(self):
        while not self.messages.empty():
            item = self.messages.get_nowait()
            if isinstance(item, int):
                self.status.set(f'Завершено, код {item}. Профиль: {self.profile}')
                self.process = None
                for button in self.buttons:
                    button.configure(state='normal')
            else:
                self.log.configure(state='normal')
                self.log.insert('end', item)
                self.log.see('end')
                self.log.configure(state='disabled')
        self.after(100, self.poll)

    def close(self):
        if self.process is not None:
            messagebox.showinfo('Процесс выполняется',
                                'Дождитесь завершения запуска. MSFS завершится по указанному таймеру.', parent=self)
            return
        self.destroy()


def main():
    parser = argparse.ArgumentParser(description='RealFlow settings launcher')
    parser.add_argument('--config', type=Path, default=Path.home() / '.realflow' / 'config.json')
    args = parser.parse_args()
    Launcher(args.config).mainloop()


if __name__ == '__main__':
    main()
