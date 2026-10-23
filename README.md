# calls

Transcripción local de grabaciones de pantalla y llamadas (por ejemplo, de OBS) con
[faster-whisper](https://github.com/SYSTRAN/faster-whisper). El audio nunca sale de tu máquina.

## Instalación

Requiere Python 3.11 o superior.

```bash
pip install -e .          # CLI
pip install -e ".[ui]"    # CLI + interfaz web
```

La primera vez que uses un modelo se descarga automáticamente (de cientos de MB a ~1,5 GB según el modelo).

## Uso

```bash
# Transcribe y deja demo.txt al lado de la grabación
calls transcribe "2026-10-23 10-00-00.mkv"

# Subtítulos y texto, en español, en otra carpeta
calls transcribe grabacion.mkv -f srt -f txt -l es -o transcripciones/

# Interfaz web en http://127.0.0.1:7860
calls ui
```

Formatos: `txt`, `srt`, `vtt` y `json` (con marcas de tiempo por segmento).

### Grabaciones de OBS con varias pistas

Si OBS graba micrófono y audio del escritorio en pistas separadas, mirá cuáles hay y elegí una:

```bash
calls tracks grabacion.mkv
# 0: aac, 2ch, 48000 Hz
# 1: aac, 1ch, 48000 Hz

calls transcribe grabacion.mkv --track 1
```

### Elegir modelo

| Modelo     | Cuándo usarlo                                             |
|------------|-----------------------------------------------------------|
| `turbo`    | Por defecto. Precisión cercana a `large-v3`, mucho más rápido. |
| `large-v3` | Máxima precisión si tenés GPU o tiempo.                   |
| `small`    | Pruebas rápidas o equipos modestos sin GPU.               |

Con GPU NVIDIA se usa CUDA automáticamente; forzá CPU con `--device cpu`.

## Desarrollo

```bash
pip install -e ".[dev,ui]"
ruff check . && ruff format --check .
pytest
```

Estructura:

- `audio.py` — lista pistas y decodifica una a 16 kHz mono (PyAV).
- `transcription.py` — `Transcriber`, envoltorio de faster-whisper que devuelve un `Transcript`.
- `formats.py` — exporta a txt/srt/vtt/json.
- `cli.py` y `ui.py` — interfaces; solo orquestan, la lógica vive en los módulos de arriba.
