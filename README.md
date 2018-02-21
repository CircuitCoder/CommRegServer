# CommRegServer
> 社团信息公开平台 - 服务端

## Installation
Install rust (through rustup.rs or any package manager), then
```bash
cd CommRegServer
cargo run
```

## Notes for developers
You may notice that this application uses a synchronize backend, Rocket. This is due to the fact that asynchronize I/O in rust is not stabilized yet, and database drivers are mostly implemented synchronize. With hyper 12.0 & Tokio 2.0 coming soon, Rocket may go asynchronize in a matter of months. To accommodate these upcoming changes, we are storing all data in LevelDB, which introduces less blocking time, making it more favorable for a synchronize backend, and makes it easier to port to other database backends as well.

### TODO
- [x] Removing files from entry
- [ ] Listing files ordered by dates
- [ ] Reordering files
- [x] Creation/Disbandment time
- [ ] Returning file names when requesting upload, making it possible to add files directly after uploading

## Maintainer
- Liu Xiaoyi <xiaoyi-l17@mails.tsinghua.edu.cn>

## License
All code under this repository is released under the MIT License. Details of the license can be found in the LICENSE file.
