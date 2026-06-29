FROM python:3.11-slim

WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY main.py .

RUN useradd -r -s /bin/false appuser \
    && chmod -R a+rX /app
USER appuser

EXPOSE 8090

CMD ["python", "main.py"]
