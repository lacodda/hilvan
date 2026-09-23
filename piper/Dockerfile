# Piper: the local voice hilvan speaks the learner's own language with.
#
# A service of its own rather than a library in the server: Piper is Python
# and ONNX, the server is one Rust binary, and the other products on the
# stand can use the same voice over HTTP. The voices are baked into the image
# rather than fetched at start, so a stand boots without Hugging Face and a
# given image version always speaks with the same voices.
#
# Measured on a Pi 4 (see the project's research note of 2026-09-23): one to
# two seconds a sentence once a voice is loaded, 150 MB for the process and
# about 130 MB for each voice in use. Voices load on first use.
FROM python:3.13-slim

# Pinned: the sound of a voice belongs to the engine version, and a cached
# sentence made by one version must not sit next to one made by another
# without anyone having decided so.
ARG PIPER_VERSION=1.8.0
RUN pip install --no-cache-dir "piper-tts[http]==${PIPER_VERSION}"

# Four Russian voices to choose the teacher from, and one English voice for
# a stand with no ElevenLabs key. Irina is the default: the one loaded first.
ARG VOICES="ru_RU-irina-medium ru_RU-denis-medium ru_RU-dmitri-medium ru_RU-ruslan-medium en_US-lessac-medium"
RUN mkdir /voices \
    && python -m piper.download_voices --data-dir /voices ${VOICES} \
    && rm -rf /root/.cache

# Unprivileged, like the server: nothing here needs root.
RUN useradd --uid 10002 --no-create-home --shell /usr/sbin/nologin piper
USER 10002
WORKDIR /voices
EXPOSE 5000

# /voices answers only once the default voice has loaded, so a container that
# answers can speak.
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://127.0.0.1:5000/voices', timeout=4)" || exit 1

CMD ["python", "-m", "piper.http_server", "--host", "0.0.0.0", "--port", "5000", "--data-dir", "/voices", "-m", "ru_RU-irina-medium"]
