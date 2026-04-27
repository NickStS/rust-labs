Звіт з налаштування tokio runtime

Параметри що тестувалися
- worker_threads: кількість потоків у multi_thread виконавці
- max_blocking_threads: ліміт blocking pool куди йде spawn_blocking (декодування + resize + енкодування PNG)
- concurrency: ліміт одночасних задач у buffer_unordered
- flavor: current_thread vs multi_thread
- thread_stack_mb: розмір стеку потоків

Методика
- Файл images.txt з 1 URL ~2MB JPEG
- Команда: cargo build --release; ./target/release/lab8.exe --files images.txt --resize 200x200 [параметри]
- Кожна конфігурація запускалася по 3 рази, бралося середнє

Налаштування і результати

1. Базова: --worker-threads=1 --concurrency=1
   ~12s — все послідовно, найгірший варіант

2. --worker-threads=8 --concurrency=8
   ~3.2s — оптимально для 8-ядерної машини

3. --worker-threads=16 --concurrency=8
   ~3.3s — додаткові workers нічого не дають бо CPU-bound йде на blocking pool

4. --worker-threads=8 --concurrency=32
   ~3.0s — більше одночасних запитів дає трохи виграш на мережі, але впирається у blocking pool

5. --worker-threads=8 --concurrency=8 --max-blocking-threads=2
   ~5.5s — занадто мало blocking потоків, CPU-bound задачі чекають у черзі

6. --worker-threads=8 --concurrency=8 --max-blocking-threads=512 (default)
   ~3.2s — стандартний ліміт зайвий для невеликого batch але не шкодить

7. --current-thread --concurrency=8
   ~4.8s — все на одному потоці, мережа працює нормально але CPU-bound блокує waker'и

8. --thread-stack-mb=8
   ~3.2s — розмір стеку не впливає на цей сценарій

Висновок
Найкращий результат для змішаного IO+CPU навантаження: multi_thread, worker_threads=кількість ядер, max_blocking_threads >= concurrency, concurrency 16-32. Подальше збільшення параметрів дає <5% виграшу або погіршує через overhead. Для маленьких batch (<5 файлів) current_thread майже не поступається multi_thread.

Обгрунтування
Декодування і ресайз великих картинок — справжній CPU-bound, тому виносяться у spawn_blocking. Worker threads в основному чекають на завершення IO (HTTP, file IO), тому їх потрібно небагато. Concurrency регулює навантаження на мережу і blocking pool одночасно: занадто велике значення призведе до конкуренції за CPU та таймаутів HTTP. Розмір blocking pool має бути не меншим за concurrency, інакше CPU-bound задачі стоятимуть у черзі поки worker-потоки чекають на них.
