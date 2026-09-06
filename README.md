# Progetto GeoRust - Gruppo G8

Workspace Cargo con 3 crate:
- `common`: struct condivisi del protocollo
- `server`: backend
- `client`: client dimostrativo (registrazione, invio posizione, messaggi)

Utenti di esempio:
- Username: user1<br>
  Password: user1
- Username: user2<br>
  Password: user2

## Come avviare

```bash
# Server
cd server
cargo run

# Client (in un altro terminale)
cd client
cargo run
```

## Indice
 - [Client](./client/)
    - ...
 - [Server](./server/)
    - ...
 - [Common](./common/)
    - [lib.rs](./common/src/lib.rs)
 - [Docs](./docs/)
    - [Report](./docs/report.md)
 - [Data](./data/)
    - ...


## Tragitti 
| Percorso | Punti | Distanza | Movimento (s)  | Fermo (s) | Velocità media |
|---|---:|---:|---:|---:|---:|
| [Torino-Asti.csv](./data/Torino-Asti.csv) | 96 | 46.54 km | 2850 | 0 | 58.79 km/h |
| [Vercelli-Novara.csv](./data/Vercelli-Novara.csv) | 33 | 20,86 km | 780 | 180 | 96,28 km/h |
| [Torino-Cuneo.csv](./data/Torino-Cuneo.csv) | 111 | 81,38 km | 3090 | 210 | 94,81 km/h |
| [Alessandria-Vercelli.csv](./data/Alessandria-Vercelli.csv) | 61 | 48,87 km | 1800 | 0 | 97,74 km/h |
| [Torino-Milano.csv](./data/Torino-Milano.csv) | 172 | 130,11 km | 4710 | 420 | 99,45 km/h |
| [Novara-Milano.csv](./data/Novara-Milano.csv) | 62 | 44,39 km | 1650 | 180 | 96,85 km/h |
| [Torino-Biella.csv](./data/Torino-Biella.csv) | 95 | 73,60 km | 2640 | 180 | 100,36 km/h |
