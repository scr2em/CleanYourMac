// Runs npkill 0.12.2's own scan pipeline without its terminal UI, as its controller does:
// worker-thread search -> per-result `du -sk | cut` (2 at a time) -> newest file of the parent.
import { from } from 'rxjs';
import { filter, map, mergeMap, switchMap, tap } from 'rxjs/operators';
import path from 'path';
const lib = new URL('./node_modules/npkill/lib/', import.meta.url).href;
const { LinuxFilesService, MacFilesService, StreamService, ConsoleService } = await import(lib + 'services/index.js');
const { FileWorkerService } = await import(lib + 'services/files/files.worker.service.js');
const { LoggerService } = await import(lib + 'services/logger.service.js');
const { SearchStatus } = await import(lib + 'models/search-state.model.js');

const root = process.argv[2];
const Files = process.platform === 'darwin' ? MacFilesService : LinuxFilesService;
const files = new Files(new StreamService(), new FileWorkerService(new LoggerService(), new SearchStatus()));
const consoleService = new ConsoleService();
const start = process.hrtime.bigint();
let found = null;
const results = [];
files.listDir({ path: root, target: 'node_modules' })
  .pipe(
    tap({ complete: () => { found = Number(process.hrtime.bigint() - start) / 1e6; } }),
    mergeMap((data) => from(consoleService.splitData(data))),
    filter((p) => p !== ''),
    map((p) => ({ path: p, size: 0, modificationTime: -1, isDangerous: files.isDangerous(p) })),
    mergeMap((folder) => files.getFolderSize(folder.path).pipe(
      tap((kb) => { folder.size = +kb; }),
      switchMap(async () => {
        if (!folder.isDangerous) {
          folder.modificationTime = await files.getRecentModificationInDir(path.join(folder.path, '../'));
        }
        return folder;
      }),
    ), 2),
    tap((folder) => results.push(folder)),
  )
  .subscribe({
    complete: () => {
      const total = Number(process.hrtime.bigint() - start) / 1e6;
      const kb = results.reduce((a, r) => a + r.size, 0);
      console.log(JSON.stringify({ found_ms: Math.round(found), total_ms: Math.round(total), results: results.length, allocated_kb: kb }));
      process.exit(0);
    },
    error: (e) => { console.error(e); process.exit(1); },
  });
