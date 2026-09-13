# GeoRust

GeoRust è un’applicazione client/server sviluppata in Rust per simulare la geolocalizzazione di una flotta di veicoli. <br>
Ogni client rappresenta un utente che può registrarsi, autenticarsi, percorrere un tragitto descritto da un file CSV e comunicare con il server. <br>
Il server riceve le posizioni, riconosce i periodi di movimento e di sosta, salva i tragitti completati e permette di consultare statistiche aggregate.

## Funzionalità principali

- registrazione e autenticazione degli utenti;
- invio periodico delle posizioni tramite WebSocket;
- simulazione ripetibile di percorsi caricati da file CSV;
- riconoscimento degli stati di movimento e di sosta;
- calcolo di distanza, durata e velocità media dei tragitti;
- statistiche giornaliere, settimanali e mensili;
- messaggi diretti e broadcast tra server e client;
- consegna differita dei messaggi diretti agli utenti non connessi;
- interfacce testuali per client e server;
- registrazione degli eventi e monitoraggio periodico della CPU del server.

## Interfacce

### Client

![Schermata di autenticazione del client](./docs/img_manuale_utente/interfaccia_iniziale_client.png)

### Server

![Dashboard del server](./docs/img_manuale_utente/interfaccia_iniziale_server.png)

## Struttura del progetto

GeoRust è organizzato come un workspace Cargo composto da tre crate:

```text
G8/
├── client/    applicazione utilizzata dagli utenti
├── server/    gestione delle connessioni, dei dati e delle statistiche
├── common/    tipi e messaggi condivisi dal protocollo
├── data/      percorsi CSV utilizzati dalla simulazione
└── docs/      manuali e documentazione del progetto
```

Il client e il server utilizzano Tokio per le attività asincrone e comunicano tramite richieste HTTP e una connessione WebSocket.<br> 
Il server è realizzato con Axum e salva utenti, tragitti e messaggi in un database SQLite tramite SQLx.<br> 
Le interfacce testuali sono costruite con Ratatui e Crossterm.<br>

## Requisiti

Per compilare ed eseguire il progetto sono necessari:

- Rust e Cargo con supporto all’edizione 2024;
- un terminale compatibile con Crossterm;
- la porta TCP `3000` disponibile sul computer che esegue il server.

Il progetto è stato verificato su Windows e Linux.<br>
SQLite è utilizzato localmente e non richiede l’installazione di un server di database separato.

## Avvio rapido

Dalla cartella principale del progetto, aprire un terminale e avviare il server:

```bash
cd server
cargo run --release
```

Aprire quindi un secondo terminale e avviare il client:

```bash
cd client
cargo run --release
```

Il server deve essere in esecuzione prima di effettuare la registrazione o il login dal client. È possibile avviare più istanze del client in terminali separati.

## Utilizzo essenziale

All’avvio, il client permette di registrare un nuovo utente premendo `r` oppure di accedere con un account esistente premendo `l`. <br>
Dopo l’autenticazione sono disponibili i seguenti comandi:

| Comando | Descrizione |
|---|---|
| `help` | Mostra la guida dei comandi. |
| `start` | Permette di scegliere un percorso CSV e avvia la simulazione. |
| `msg <testo>` | Invia un messaggio al server. |
| `exit` | Chiude il client. |

La console del server mette a disposizione questi comandi:

| Comando | Descrizione |
|---|---|
| `help` | Mostra la guida dei comandi. |
| `users` | Elenca gli utenti registrati. |
| `logs <username>` | Mostra fino a 10 messaggi associati all’utente. |
| `stats <username>` | Consulta le statistiche dei tragitti dell’utente. |
| `msg` | Avvia l’invio guidato di un messaggio diretto o broadcast. |
| `exit` | Arresta il server. |

Per la descrizione completa delle interfacce e delle operazioni disponibili consultare il [manuale utente](./docs/manuale_utente.md).

## Percorsi disponibili

I file nella cartella [`data`](./data/) contengono le coordinate e i tempi logici utilizzati dalla simulazione.<br> 
Ogni nuova posizione è associata a un intervallo logico di 30 secondi.

| Percorso | Punti | Durata (min) | Distanza | Movimento (s) | Sosta (s) | Velocità media |
|---|---:|:---:|---:|:---:|:---:|---:|
| [Torino–Asti](./data/Torino-Asti.csv) | 96 | 47,5 | 46,54 km | 2.850 | 0 | 58,79 km/h |
| [Vercelli–Novara](./data/Vercelli-Novara.csv) | 33 | 16 | 20,86 km | 780 | 180 | 96,28 km/h |
| [Torino–Cuneo](./data/Torino-Cuneo.csv) | 111 | 55 | 81,38 km | 3.090 | 210 | 94,81 km/h |
| [Alessandria–Vercelli](./data/Alessandria-Vercelli.csv) | 61 | 30 | 48,87 km | 1.800 | 0 | 97,74 km/h |
| [Torino–Milano](./data/Torino-Milano.csv) | 172 | 85,5 | 130,11 km | 4.710 | 420 | 99,45 km/h |
| [Novara–Milano](./data/Novara-Milano.csv) | 62 | 30,5 | 44,39 km | 1.650 | 180 | 96,85 km/h |
| [Torino–Biella](./data/Torino-Biella.csv) | 95 | 47 | 73,60 km | 2.640 | 180 | 100,36 km/h |

## Documentazione

- [Manuale utente](./docs/manuale_utente.md): installazione, avvio e utilizzo dell’applicazione.
- [Manuale del progettista](./docs/manuale_progettista.md): architettura, scelte implementative, analisi e valutazione del progetto.
- [Verifiche su Linux](./docs/tests_on_Linux.md): controlli eseguiti in ambiente Linux.
