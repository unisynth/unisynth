// Minimal Win32 shim (pthreads) so upstream WaveSabre's SongRenderer and samplers build natively on macOS for tools/ws-parity. Ours (MIT); no upstream code.
#pragma once
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
typedef unsigned char byte;
typedef uint32_t DWORD; typedef int BOOL; typedef void *LPVOID; typedef uint8_t BYTE; typedef uint16_t WORD;
typedef int32_t LONG; typedef uintptr_t UINT_PTR; typedef DWORD *LPDWORD; typedef uintptr_t DWORD_PTR;
#define WINAPI
#define FALSE 0
#define TRUE 1
#define __stdcall
#define INFINITE 0xFFFFFFFFu
#define THREAD_PRIORITY_HIGHEST 2
#pragma pack(push, 1)
typedef struct { WORD wFormatTag, nChannels; DWORD nSamplesPerSec, nAvgBytesPerSec; WORD nBlockAlign, wBitsPerSample, cbSize; } WAVEFORMATEX;
#pragma pack(pop)
static_assert(sizeof(WAVEFORMATEX) == 18, "WAVEFORMATEX must be packed");
typedef WAVEFORMATEX *LPWAVEFORMATEX;
#define WAVE_FORMAT_UNKNOWN 0
#define WAVE_FORMAT_PCM 1
#define WAVE_FORMAT_GSM610 0x31
struct ShimHandle { int kind; pthread_mutex_t m; pthread_cond_t c; bool sig; pthread_t t; };
typedef ShimHandle *HANDLE;
typedef DWORD (*LPTHREAD_START_ROUTINE)(LPVOID);
inline HANDLE CreateEvent(void*, BOOL, BOOL initial, void*) {
  HANDLE h = new ShimHandle(); h->kind = 0; pthread_mutex_init(&h->m, 0); pthread_cond_init(&h->c, 0); h->sig = initial; return h; }
inline BOOL SetEvent(HANDLE h) { pthread_mutex_lock(&h->m); h->sig = true; pthread_cond_signal(&h->c); pthread_mutex_unlock(&h->m); return 1; }
inline DWORD WaitForSingleObject(HANDLE h, DWORD) {
  if (h->kind == 1) { pthread_join(h->t, 0); return 0; }
  pthread_mutex_lock(&h->m); while (!h->sig) pthread_cond_wait(&h->c, &h->m); h->sig = false; pthread_mutex_unlock(&h->m); return 0; }
inline DWORD WaitForMultipleObjects(DWORD n, HANDLE *hs, BOOL, DWORD t) { for (DWORD i = 0; i < n; i++) WaitForSingleObject(hs[i], t); return 0; }
struct ShimThreadArg { LPTHREAD_START_ROUTINE f; LPVOID p; };
inline void *shimThreadMain(void *a) { auto s = (ShimThreadArg*)a; s->f(s->p); delete s; return 0; }
inline HANDLE CreateThread(void*, size_t, LPTHREAD_START_ROUTINE f, LPVOID p, DWORD, LPDWORD) {
  HANDLE h = new ShimHandle(); h->kind = 1; pthread_create(&h->t, 0, shimThreadMain, new ShimThreadArg{f, p}); return h; }
inline BOOL SetThreadPriority(HANDLE, int) { return 1; }
inline BOOL CloseHandle(HANDLE h) { if (h && h->kind == 0) { pthread_mutex_destroy(&h->m); pthread_cond_destroy(&h->c); } delete h; return 1; }
typedef pthread_mutex_t CRITICAL_SECTION;
inline void InitializeCriticalSection(CRITICAL_SECTION *c) { pthread_mutex_init(c, 0); }
inline void DeleteCriticalSection(CRITICAL_SECTION *c) { pthread_mutex_destroy(c); }
inline void EnterCriticalSection(CRITICAL_SECTION *c) { pthread_mutex_lock(c); }
inline void LeaveCriticalSection(CRITICAL_SECTION *c) { pthread_mutex_unlock(c); }
inline unsigned int InterlockedCompareExchange(volatile unsigned int *d, unsigned int x, unsigned int cmp) { return __sync_val_compare_and_swap(d, cmp, x); }
template <class T> inline T InterlockedDecrement(volatile T *v) { return __sync_sub_and_fetch(v, 1); }
typedef void *HACMDRIVERID; typedef void *HACMDRIVER; typedef void *HACMSTREAM;
struct ACMFORMATDETAILS; typedef ACMFORMATDETAILS *LPACMFORMATDETAILS;
