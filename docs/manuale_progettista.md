# GeoRust - Manuale del progettista <!-- omit in toc -->

## Indice <!-- omit in toc -->

- [1. Introduzione](#1-introduzione)
- [2. Analisi dei requisiti](#2-analisi-dei-requisiti)
- [3. Architettura del sistema](#3-architettura-del-sistema)
  - [3.1 Crate `common`](#31-crate-common)
  - [3.2 Crate `client`](#32-crate-client)
  - [3.3 Crate `server`](#33-crate-server)
- [4. Comunicazione tra client e server](#4-comunicazione-tra-client-e-server)
  - [4.1 Registrazione e autenticazione](#41-registrazione-e-autenticazione)
  - [4.2 Apertura della WebSocket](#42-apertura-della-websocket)
  - [4.3 Messaggi scambiati](#43-messaggi-scambiati)
  - [4.4 Ciclo della connessione](#44-ciclo-della-connessione)
- [5. Simulazione del movimento](#5-simulazione-del-movimento)
- [6. Macchina a stati del tragitto](#6-macchina-a-stati-del-tragitto)
  - [6.1 Transizioni di stato](#61-transizioni-di-stato)
  - [6.2 Calcolo della distanza](#62-calcolo-della-distanza)
- [7. Persistenza e modello dei dati](#7-persistenza-e-modello-dei-dati)
  - [7.1 Struttura del database](#71-struttura-del-database)
  - [7.2 Salvataggio dei tragitti](#72-salvataggio-dei-tragitti)
  - [7.3 Elaborazione delle statistiche](#73-elaborazione-delle-statistiche)
- [8. Sistema di messaggistica](#8-sistema-di-messaggistica)
  - [8.1 Gestione delle connessioni e canali Tokio](#81-gestione-delle-connessioni-e-canali-tokio)
  - [8.2 Persistenza e consegna](#82-persistenza-e-consegna)
  - [8.3 Validazione e gestione degli errori](#83-validazione-e-gestione-degli-errori)
- [9. Interfacce testuali](#9-interfacce-testuali)
  - [9.1 Interfaccia del client](#91-interfaccia-del-client)
  - [9.2 Interfaccia del server](#92-interfaccia-del-server)
  - [9.3 Gestione del terminale e degli eventi](#93-gestione-del-terminale-e-degli-eventi)
- [10. Verifica e test](#10-verifica-e-test)
- [11. Prestazioni e dimensione degli eseguibili](#11-prestazioni-e-dimensione-degli-eseguibili)
  - [11.1 Registrazione dell’utilizzo della CPU](#111-registrazione-dellutilizzo-della-cpu)
  - [11.2 Scelte relative alle prestazioni](#112-scelte-relative-alle-prestazioni)
  - [11.3 Dimensione degli eseguibili](#113-dimensione-degli-eseguibili)
- [12. Valutazione finale](#12-valutazione-finale)

## 1. Introduzione

GeoRust è un’applicazione client/server sviluppata in Rust per simulare la geolocalizzazione e la comunicazione di una flotta di veicoli. Ogni client rappresenta un utente registrato che, dopo l’autenticazione, può avviare un tragitto, trasmettere periodicamente la propria posizione e inviare messaggi al server.

Il movimento viene simulato leggendo una sequenza di coordinate da un file CSV. Ogni posizione è associata a un tempo logico e viene inviata al server rispettando intervalli logici di 30 secondi. Durante il tragitto, il server determina lo stato dell’utente, calcola la distanza percorsa e distingue il tempo trascorso in movimento da quello trascorso in pausa.

Il server gestisce inoltre la registrazione e l’autenticazione degli utenti, la persistenza dei dati in un database SQLite, l’elaborazione delle statistiche e lo scambio di messaggi diretti o broadcast. Sia il client sia il server dispongono di un’interfaccia testuale realizzata con Ratatui e Crossterm.

L’applicazione utilizza Tokio per coordinare le attività che devono procedere contemporaneamente. Sul server vengono gestite le richieste HTTP, le connessioni WebSocket, la console amministrativa e il logger della CPU; sul client il ciclo di gestione degli eventi coordina l’input dell’utente, la simulazione del percorso, la ricezione dei messaggi e i tentativi di riconnessione, senza bloccare l’interfaccia.

## 2. Analisi dei requisiti

| Requisito | Soluzione adottata |
|---|---|
| Registrazione degli utenti tramite account e password | Il server espone due API REST per la registrazione e il login. Le password vengono salvate sotto forma di hash generato con Argon2. |
| Geolocalizzazione degli utenti | Il client legge le coordinate da percorsi CSV e le invia al server attraverso una connessione WebSocket. |
| Invio della posizione ogni 30 secondi | Ogni punto del percorso contiene un tempo logico. Il client e il server verificano che le posizioni siano distanziate esattamente di 30 secondi. |
| Gestione degli stati dell’utente | Il server utilizza gli stati `Disconnected`, `Moving` e `Still`. Il passaggio a `Moving` avviene al primo cambiamento di coordinate, mentre il passaggio a `Still` avviene dopo tre minuti senza variazioni delle coordinate inviate. |
| Analisi del movimento | Per ogni tragitto completato vengono calcolati e salvati la distanza percorsa, il tempo di movimento e il tempo di pausa. La velocità media viene ricavata dalle statistiche aggregate del periodo selezionato. |
| Intervalli temporali programmabili | La console amministrativa del server consente di richiedere le statistiche relative al giorno, alla settimana o al mese corrente di uno specifico utente. |
| Comunicazione con gli utenti | Il server può inviare messaggi diretti oppure broadcast ai client. I client possono inviare messaggi testuali solamente al server. |
| Controllo del consumo di CPU | Il server registra ogni 120 secondi l’utilizzo della CPU, una stima del tempo CPU impiegato nell’ultimo intervallo e la stima cumulativa dall’avvio del processo. |
| Esecuzione su almeno due piattaforme | La compilazione e i controlli automatici vengono eseguiti tramite GitHub Actions su Windows e Ubuntu. |

Per rendere la simulazione ripetibile è stata scelta l’emulazione tramite file CSV. Gli stessi percorsi possono così essere eseguiti più volte e produrre risultati confrontabili. Nelle build di debug è inoltre disponibile un *fattore di velocità* che riduce i tempi di attesa reali senza modificare i tempi logici inviati al server.

I dati relativi agli utenti, ai tragitti e ai messaggi vengono salvati in SQLite. L’elenco degli utenti online viene invece ricavato dalle connessioni WebSocket attive. Anche i token di autenticazione sono conservati in memoria e vengono quindi persi al riavvio del server.

## 3. Architettura del sistema

GeoRust è organizzato come un workspace Cargo composto da tre crate: `common`, `client` e `server`. Questa suddivisione permette di separare le responsabilità principali e di condividere tra client e server i tipi comuni utilizzati nella comunicazione.

### 3.1 Crate `common`

Il crate `common` contiene i tipi utilizzati da entrambe le applicazioni: al suo interno sono definiti i dati scambiati durante la registrazione e il login, i messaggi del protocollo WebSocket, gli stati dell’utente e il tipo che rappresenta una coordinata geografica. La condivisione di questi tipi riduce il rischio che client e server interpretino diversamente lo stesso messaggio.

| File | Responsabilità |
|---|---|
| `common/src/lib.rs` | Definisce i dati condivisi tra client e server: richieste e risposte REST, stati dell’utente e messaggi del protocollo WebSocket. Contiene inoltre l’intervallo logico di 30 secondi utilizzato per le posizioni. |
| `common/src/tracking.rs` | Definisce il tipo `Coordinata` e verifica che latitudine e longitudine appartengano agli intervalli geografici validi. La validazione viene applicata anche durante la deserializzazione dei messaggi ricevuti. |

### 3.2 Crate `client`

Il crate `client` gestisce l’interazione con l’utente: si occupa dell’autenticazione tramite richieste HTTP, della selezione e validazione dei percorsi CSV, della simulazione temporale del movimento e della comunicazione con il server attraverso WebSocket.

| File | Responsabilità |
|---|---|
| `client/src/main.rs` | Avvia la console del client. |
| `client/src/console/mod.rs` | Dichiara i moduli della console e ne espone il punto di ingresso. |
| `client/src/console/auth.rs` | Gestisce l’interfaccia di registrazione e login e invia le relative richieste REST al server. |
| `client/src/console/app.rs` | Contiene lo stato dell’interfaccia, interpreta i comandi dell’utente e gestisce la selezione dei percorsi disponibili. |
| `client/src/console/runtime.rs` | Coordina gli eventi della tastiera, la connessione WebSocket, il simulatore e i tentativi di riconnessione. |
| `client/src/console/ui.rs` | Definisce il layout e il rendering dell’interfaccia testuale tramite Ratatui. |
| `client/src/movement/mod.rs` | Definisce il punto di un percorso e rende disponibili il caricamento dei CSV e il simulatore. |
| `client/src/movement/csv_route.rs` | Legge i percorsi CSV e ne verifica formato, intervalli temporali e coordinate. |
| `client/src/movement/simulator.rs` | Riproduce temporalmente i punti del percorso e li rende disponibili al ciclo di gestione del client. |

### 3.3 Crate `server`

Il crate `server` espone le API REST e l’endpoint WebSocket, autentica le connessioni dei client, mantiene l’elenco degli utenti collegati, gestisce i tragitti attivi e salva nel database quelli completati. Si occupa inoltre delle statistiche, della messaggistica, della console amministrativa e della registrazione periodica dell’utilizzo della CPU.

| File | Responsabilità |
|---|---|
| `server/src/main.rs` | Inizializza il database e lo stato condiviso, configura le API REST e la WebSocket e avvia il server e la console amministrativa. |
| `server/src/auth.rs` | Implementa registrazione e login, hashing delle password e generazione dei token di autenticazione. |
| `server/src/state.rs` | Contiene lo stato condiviso dell’applicazione e gestisce i tragitti attivi, l’accesso ai dati e il salvataggio transazionale dei tragitti completati. |
| `server/src/trip.rs` | Implementa la macchina a stati del tragitto e calcola tempi di movimento, tempi di pausa e distanza percorsa. |
| `server/src/stats.rs` | Calcola le statistiche giornaliere, settimanali e mensili a partire dai tragitti salvati. |
| `server/src/messaging.rs` | Gestisce la persistenza e la consegna dei messaggi diretti e broadcast, oltre all’elenco degli utenti connessi. |
| `server/src/cpu_usage.rs` | Registra periodicamente l’utilizzo della CPU del processo server. |
| `server/src/console/mod.rs` | Dichiara i moduli della console amministrativa e ne espone il punto di ingresso. |
| `server/src/console/app.rs` | Contiene lo stato della console e interpreta i comandi dell’amministratore. |
| `server/src/console/runtime.rs` | Coordina gli eventi della console e richiama le funzionalità relative a utenti, messaggi e statistiche. |
| `server/src/console/tui.rs` | Definisce il layout e il rendering della console amministrativa. |

Lo stato utilizzato dagli handler di Axum è raccolto in `AppState` e condiviso tramite `Arc`, così le diverse attività possono accedere alla stessa istanza senza trasferirne la proprietà. Le mappe modificabili durante l’esecuzione sono protette da `Mutex` o `RwLock`: i token, i tragitti attivi e le connessioni degli utenti possono quindi essere consultati e aggiornati in modo controllato dalle operazioni concorrenti. Il pool SQLite è gestito da SQLx e viene condiviso con i servizi che accedono al database.

## 4. Comunicazione tra client e server

La comunicazione tra client e server avviene in due fasi. Le operazioni iniziali di registrazione e login utilizzano richieste REST su HTTP, mentre le posizioni e i messaggi vengono scambiati attraverso una connessione WebSocket persistente.

### 4.1 Registrazione e autenticazione

Il server espone gli endpoint `POST /register` e `POST /login`. La registrazione riceve username e password, verifica che i dati non siano vuoti e salva la password sotto forma di hash Argon2 generato con un salt casuale. Dopo una registrazione completata, il client esegue automaticamente il login.

Durante il login il server confronta la password ricevuta con l’hash memorizzato nel database. Se le credenziali sono valide, genera un token UUID e lo associa all’identificativo dell’utente. Il token viene mantenuto in memoria dal server e restituito al client insieme allo `user_id`.

### 4.2 Apertura della WebSocket

Dopo l’autenticazione, il client apre una connessione verso l’endpoint `GET /ws` e inserisce il token nell’header `Authorization` come token Bearer. Il server verifica il token prima di accettare l’aggiornamento della connessione a WebSocket. Una richiesta priva di un token valido viene rifiutata, così come il tentativo di aprire una seconda WebSocket per un utente già connesso.

La WebSocket permette una comunicazione bidirezionale: il server può inviare informazioni al client senza attendere una nuova richiesta HTTP e, nello stesso tempo, continuare a ricevere posizioni e messaggi dall’utente. Il client conserva il token ottenuto durante il login e lo riutilizza negli eventuali tentativi di riconnessione.

### 4.3 Messaggi scambiati

I messaggi applicativi sono definiti nel crate `common` e vengono convertiti in JSON tramite Serde. In questo modo client e server utilizzano la stessa rappresentazione dei dati.

I principali messaggi inviati dal client sono:

| Messaggio | Funzione |
|---|---|
| `StartTrip` | Richiede la creazione di un nuovo tragitto. |
| `PositionUpdate` | Invia una coordinata e il relativo tempo logico. |
| `TripCompleted` | Comunica che tutti i punti del percorso sono stati trasmessi. |
| `Text` | Invia un messaggio testuale al server. |
| `DirectTextAck` | Conferma la ricezione di un messaggio diretto. |

I principali messaggi inviati dal server sono:

| Messaggio | Funzione |
|---|---|
| `TripStarted` | Conferma la creazione del tragitto. |
| `PositionAccepted` | Conferma una posizione e comunica lo stato corrente dell’utente. |
| `TripCompleted` | Restituisce il riepilogo del tragitto completato. |
| `DirectText` | Contiene un messaggio destinato a uno specifico utente. |
| `BroadcastText` | Contiene un messaggio inviato a tutti gli utenti connessi. |
| `Error` | Comunica un errore attraverso un codice e un messaggio descrittivo. |

### 4.4 Ciclo della connessione

Dopo l’apertura della WebSocket, il client può scambiare messaggi con il server anche mentre è in corso un tragitto. Quando il percorso termina regolarmente, il client invia `TripCompleted`; il server conclude il tragitto, salva i dati e restituisce un riepilogo. Successivamente chiude la WebSocket e il client tenta automaticamente di riconnettersi utilizzando lo stesso token.

Se la connessione viene interrotta durante la simulazione, il client arresta il simulatore e non invia il messaggio di completamento. Il server rimuove l’utente dall’elenco dei client connessi e scarta l’eventuale tragitto ancora attivo, evitando di salvare un viaggio incompleto.

## 5. Simulazione del movimento

Il movimento di un utente viene simulato attraverso percorsi memorizzati in file CSV nella cartella `data`. Ogni file rappresenta un tragitto ed è composto dalle colonne `time`, `latitude` e `longitude`: il tempo indica quando deve essere inviata una posizione rispetto all’inizio del viaggio, mentre latitudine e longitudine identificano la coordinata geografica. I percorsi disponibili vengono individuati automaticamente dall’interfaccia client e possono essere selezionati tramite il loro numero o il nome del percorso.

Prima di avviare la simulazione, il client legge e valida l’intero percorso. Il file deve contenere almeno un punto, la prima posizione deve essere associata al tempo `00:00` e tutte le successive devono essere distanziate esattamente di 30 secondi. Viene inoltre verificata la validità delle coordinate geografiche. Sono ammesse coordinate consecutive uguali, purché rispettino l’intervallo temporale di 30 secondi. Questa situazione viene utilizzata per rappresentare i periodi in cui il veicolo rimane fermo.

Dopo la validazione, il client invia al server il messaggio `StartTrip` e avvia il simulatore. Il primo punto viene trasmesso immediatamente; per ciascun punto successivo il simulatore attende il tempo previsto e lo invia, tramite un canale asincrono, al ciclo di gestione del client. Questo componente gestisce gli eventi provenienti dalla tastiera, dal simulatore e dalla connessione WebSocket; quando riceve una nuova posizione la converte in un messaggio `PositionUpdate` e la trasmette al server insieme al tempo logico trascorso, che non viene modificato dal fattore di velocità.

Anche il server verifica i tempi logici contenuti negli aggiornamenti: il primo punto deve avere tempo zero e ogni punto successivo deve avanzare di 30 secondi. In questo modo non si affida esclusivamente alla validazione eseguita dal client.

Nelle build di debug è possibile scegliere un fattore di velocità che riduce il tempo di attesa reale tra due posizioni. Con un fattore `60`, ad esempio, i 30 secondi previsti dal percorso vengono riprodotti in 500 millisecondi. Questa accelerazione riguarda solamente l’esecuzione della simulazione: i tempi logici inviati al server rimangono invariati. Nelle build release il fattore è impostato automaticamente a `1` e le posizioni vengono quindi inviate in tempo reale ogni 30 secondi.

Quando tutti i punti sono stati trasmessi, il client invia `TripCompleted`, permettendo al server di concludere e salvare il tragitto. Se il client viene chiuso o perde la connessione prima della fine, il simulatore viene arrestato e il messaggio di completamento non viene inviato; il server scarta quindi il tragitto interrotto senza salvarlo nel database.

## 6. Macchina a stati del tragitto

La logica di un tragitto è implementata nel tipo `Trip`. Ogni istanza è associata all’identificativo dell’utente e conserva le posizioni ricevute, lo stato corrente, il tempo di movimento, il tempo di pausa e l’eventuale periodo di immobilità che non ha ancora raggiunto la soglia dei tre minuti.

### 6.1 Transizioni di stato

Un nuovo tragitto nasce nello stato `Disconnected`. La prima posizione deve avere tempo logico pari a zero e porta lo stato a `Still`, senza aggiungere secondi al tempo di pausa. Da quel momento lo stato viene aggiornato confrontando ogni coordinata con quella precedente.

| Stato corrente | Evento | Risultato |
|---|---|---|
| `Still` | La coordinata non cambia | Lo stato rimane `Still` e i 30 secondi trascorsi vengono aggiunti al tempo di pausa. |
| `Still` | La coordinata cambia | Lo stato passa a `Moving` e l’intervallo viene aggiunto al tempo di movimento. |
| `Moving` | La coordinata cambia | Lo stato rimane `Moving` e viene aggiornato il tempo di movimento. |
| `Moving` | La coordinata non cambia per meno di tre minuti | Il tempo trascorso viene mantenuto temporaneamente in attesa di conoscere la posizione successiva. |
| `Moving` | La coordinata non cambia per almeno tre minuti | Lo stato passa a `Still` e l’intero periodo di immobilità viene attribuito al tempo di pausa. |

La durata in cui un utente in movimento mantiene la stessa coordinata viene accumulata in `pending_still_time`. Se il movimento riprende prima di 180 secondi, questo periodo non costituisce una pausa secondo la definizione adottata e viene sommato al tempo di movimento. Se invece si raggiungono i 180 secondi, il periodo viene assegnato retroattivamente al tempo di pausa e lo stato diventa `Still`.

Al completamento del tragitto lo stato torna a `Disconnected`. Un eventuale periodo di immobilità inferiore alla soglia viene conteggiato come movimento prima di produrre il riepilogo finale.

### 6.2 Calcolo della distanza

La distanza totale viene calcolata sommando la distanza tra ogni coppia di coordinate consecutive. Per ciascun segmento viene applicata la formula di Haversine, utilizzando un raggio terrestre di 6.371 km. Le coordinate ripetute producono un segmento di lunghezza nulla, mentre il risultato complessivo viene arrotondato a due cifre decimali.

## 7. Persistenza e modello dei dati

La persistenza è affidata a SQLite tramite il crate SQLx. All’avvio il server apre il file `db.sqlite` in modalità lettura e scrittura e crea, se non sono già presenti, le tabelle e l’indice necessari. Il file deve quindi esistere prima dell’avvio del server, mentre lo schema viene inizializzato dal programma.

### 7.1 Struttura del database

| Tabella | Contenuto |
|---|---|
| `users` | Identificativo, username, hash della password e colonna `current_state`. |
| `trips` | Utente, data del tragitto, distanza totale, secondi di movimento e secondi di pausa. |
| `trips_points` | Indice del punto, tempo logico e coordinate associate a un tragitto. |
| `messages` | Mittente, destinatario, tipo, contenuto, data di creazione e stato di lettura del messaggio. |

Le tabelle `trips` e `messages` fanno riferimento agli utenti registrati. Ogni riga di `trips_points` è invece collegata a un tragitto tramite il suo identificativo e utilizza, insieme all’indice del punto, una chiave primaria composta. Sono presenti vincoli sui valori numerici e sulle coordinate, oltre alle chiavi esterne abilitate durante l’apertura del database.

La colonna `users.current_state` fa parte dello schema, ma non viene utilizzata per stabilire quali utenti siano online. Questa informazione viene ricavata dalla mappa delle connessioni WebSocket attive, in modo che la console mostri lo stato effettivo delle connessioni presenti in quel momento.

### 7.2 Salvataggio dei tragitti

I tragitti in corso vengono mantenuti in memoria all’interno di `AppState`. Quando il server riceve `TripCompleted`, produce il riepilogo e salva prima i dati aggregati nella tabella `trips`, quindi inserisce tutte le coordinate nella tabella `trips_points`. Le operazioni vengono eseguite nella stessa transazione SQL: se uno degli inserimenti fallisce, la transazione non viene completata e non rimangono dati parziali nel database.

Dopo un salvataggio riuscito il tragitto viene rimosso dalla memoria. I punti restano disponibili nel database per una futura ricostruzione del percorso, anche se l’attuale console amministrativa utilizza soltanto i valori aggregati presenti in `trips`. Un tragitto interrotto senza il messaggio di completamento viene invece eliminato dalla memoria senza essere salvato.

### 7.3 Elaborazione delle statistiche

La console amministrativa permette di interrogare le statistiche di uno specifico utente per il giorno, la settimana o il mese corrente. Gli intervalli sono determinati a partire dalla data UTC del server: per la settimana viene considerato come inizio il lunedì, mentre per il mese viene utilizzato il primo giorno. Le query sommano la distanza, il tempo di movimento e il tempo di pausa dei tragitti compresi nel periodo selezionato.

La velocità media viene calcolata dividendo la distanza complessiva per il solo tempo trascorso in movimento e convertendo il risultato in chilometri orari. Le pause non entrano quindi nel denominatore; se non è presente alcun tempo di movimento, il valore restituito è zero.

## 8. Sistema di messaggistica

Il sistema di messaggistica permette alla console amministrativa e ai client connessi di comunicare tra loro. La console può inviare un messaggio diretto a un singolo utente oppure in broadcast verso tutti gli utenti connessi; il client, invece, può inviare testo soltanto al server. La comunicazione utilizza la connessione WebSocket.

### 8.1 Gestione delle connessioni e canali Tokio

Il server memorizza in `AppState` un’istanza di `MessageService`. Quando un client apre una connessione WebSocket, l’handler della socket chiede al servizio di associare la connessione all’utente. Il servizio crea un canale `tokio::sync::mpsc` per quel client e memorizza il `Sender` in una mappa indicizzata dall’identificativo dell’utente; il `Receiver` viene invece restituito all’handler della socket.

Per i messaggi broadcast viene usato un canale `tokio::sync::broadcast`. Ogni connessione possiede una propria sottoscrizione e riceve gli eventi pubblicati sul canale.

Quando la connessione WebSocket è attiva, l’handler ascolta sia i messaggi in ingresso sulla socket sia quelli pubblicati sul canale `mpsc` dell’utente e sul canale broadcast, utilizzando `tokio::select!`. I messaggi ricevuti dai canali vengono serializzati in JSON tramite Serde e inviati sulla WebSocket. Quando la connessione termina, il client viene rimosso dal registro.

### 8.2 Persistenza e consegna

Tutti i messaggi vengono memorizzati nella tabella `messages` di SQLite con il tipo, il testo, la data di creazione, gli identificativi di mittente e destinatario e un flag di lettura. Il tipo può essere `client_to_server`, `direct` o `broadcast`.

Un messaggio diretto viene inizialmente marcato come non letto. Quando il client lo riceve, invia una conferma `DirectTextAck` con l’identificativo del messaggio; il server aggiorna quindi il relativo flag di lettura. Se il destinatario è offline, il record rimane non letto e viene recuperato alla connessione successiva insieme agli altri messaggi pendenti, in ordine cronologico. I broadcast vengono invece consegnati agli utenti collegati al momento della pubblicazione e non vengono riproposti a chi era offline.

La console accede periodicamente alla tabella per mostrare all’amministratore gli ultimi 100 messaggi inviati e ricevuti.

### 8.3 Validazione e gestione degli errori

I messaggi inviati dal client al server non possono essere vuoti né superare il limite di lunghezza previsto dal servizio. Anche i messaggi creati dalla console amministrativa devono contenere testo e, nel caso di un invio diretto, il destinatario deve corrispondere a un utente registrato. Il servizio può produrre gli errori `ValidationError`, `NotFound` e `DatabaseError`. Gli errori di validazione vengono restituiti al client con il codice `validation_error`, mentre gli errori SQLite vengono registrati nei log e comunicati all’esterno con il codice generico `internal_error`.

Un errore durante la serializzazione o l’invio di un messaggio al client interrompe la connessione; i messaggi diretti già salvati rimangono comunque disponibili per un tentativo di consegna successivo.

## 9. Interfacce testuali

Il client e il server dispongono di interfacce testuali costruite con Ratatui e Crossterm. In entrambi i casi il codice è diviso tra stato dell’applicazione, gestione degli eventi e rendering: i moduli `app.rs` interpretano i comandi e aggiornano lo stato, i moduli `runtime.rs` coordinano le operazioni asincrone e `ui.rs` o `tui.rs` definiscono la disposizione degli elementi sul terminale.

### 9.1 Interfaccia del client

All’avvio il client mostra una schermata di autenticazione dalla quale è possibile effettuare il login o registrare un nuovo account. La password viene mascherata durante l’inserimento e, al termine di una registrazione, viene eseguito automaticamente il login.

Dopo l’autenticazione viene visualizzata la dashboard principale. L’intestazione mostra l’utente e lo stato della connessione; la parte centrale è divisa tra la console, che occupa il 60% dello spazio, e l’elenco dei messaggi, che occupa il restante 40%. La riga di input rimane disponibile anche durante un tragitto, permettendo all’utente di continuare a inviare messaggi al server.

I comandi disponibili sono `help`, `msg <testo>`, `start` ed `exit`. Il comando `start` avvia la scelta del percorso e, nelle build di debug, del fattore di velocità. I messaggi diretti, i broadcast e i messaggi inviati vengono rappresentati con stili differenti, mentre lo scorrimento della console e dell’elenco dei messaggi può essere controllato tramite tastiera o rotellina del mouse.

### 9.2 Interfaccia del server

La console amministrativa viene eseguita nello stesso processo del server. La schermata è divisa in una console per i comandi, un pannello con i messaggi e un elenco degli utenti online. Questi ultimi vengono ricavati dalle connessioni WebSocket attive, mentre la cronologia mostrata nel pannello dei messaggi viene aggiornata ogni secondo.

L’amministratore può visualizzare gli utenti registrati con `users`, consultare fino a 10 messaggi associati a un utente con `logs <username>`, richiedere le statistiche con `stats <username>` e inviare messaggi diretti o broadcast tramite `msg`. Il comando `exit` chiude la console e attiva l’arresto controllato del server HTTP.

### 9.3 Gestione del terminale e degli eventi

Crossterm abilita la modalità raw, lo schermo alternativo e la lettura asincrona di tastiera e mouse tramite `EventStream`. Le interfacce utilizzano `tokio::select!` per attendere contemporaneamente gli eventi del terminale e quelli prodotti dalle altre attività dell’applicazione. Una struttura `TerminalSession` si occupa dell’inizializzazione e implementa `Drop` per ripristinare il terminale anche quando l’esecuzione termina a causa di un errore.

## 10. Verifica e test

I test automatici sono distribuiti nei tre crate e verificano soprattutto le parti che contengono regole applicative. Il crate `common` controlla la validazione delle coordinate e la serializzazione dei messaggi; il client verifica la lettura dei CSV, la temporizzazione del simulatore e l’interpretazione dei comandi; il server copre la macchina a stati, il calcolo delle statistiche, la persistenza dei tragitti e il servizio di messaggistica.

I test che richiedono SQLite utilizzano database in memoria, evitando di modificare il file impiegato dall’applicazione. Per verificare il simulatore vengono inoltre usati i controlli temporali di Tokio, che permettono di avanzare il tempo dei test senza attendere realmente tutti gli intervalli del percorso.

La suite corrente comprende 59 test:

| Crate | Test |
|---|---:|
| `common` | 8 |
| `client` | 25 |
| `server` | 26 |

I controlli locali utilizzati per la revisione del workspace sono:

```text
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

La pipeline GitHub Actions esegue questi controlli su `windows-latest` e `ubuntu-latest` per push e pull request relativi ai branch `dev` e `main`, oltre a consentire l’avvio manuale. I job delle due piattaforme sono indipendenti e utilizzano una cache per le dipendenze e la cartella `target`. La build release completa viene eseguita per i push su `main` e per le pull request dirette a `main`; quando parte una nuova esecuzione sullo stesso riferimento, quella precedente viene annullata per evitare lavoro duplicato.

Non sono presenti test di integrazione che avviano realmente server e client come processi separati. Le interazioni complete, come la riconnessione, il rifiuto delle connessioni duplicate e la consegna dei messaggi tra più client, richiedono quindi anche una verifica manuale.

## 11. Prestazioni e dimensione degli eseguibili

### 11.1 Registrazione dell’utilizzo della CPU

All’avvio del server viene avviato un task dedicato al monitoraggio della CPU. Ogni 120 secondi il modulo `cpu_usage.rs` interroga `sysinfo` per ottenere l’utilizzo del processo e aggiunge una riga al file `logs/cpu_usage_log.txt`. Ogni registrazione contiene data e ora, PID, percentuale di utilizzo, uptime del processo, stima del tempo CPU impiegato nell’ultimo intervallo e stima cumulativa dall’avvio del logger.

Il tempo relativo all’intervallo viene stimato moltiplicando la percentuale di utilizzo per i 120 secondi trascorsi. Il valore cumulativo è la somma delle stime prodotte nei diversi intervalli: non rappresenta quindi una misura esatta fornita dal sistema operativo, ma un’indicazione dell’attività del processo nel corso dell’esecuzione.

Oltre al monitoraggio della CPU, il server utilizza `tracing` per registrare nella cartella `logs` gli eventi relativi alle connessioni WebSocket, agli errori e al completamento dei tragitti. Questi log vengono organizzati su base giornaliera.

### 11.2 Scelte relative alle prestazioni

Le operazioni di rete e di accesso ai dati sono asincrone, così una connessione in attesa non blocca l’elaborazione delle altre. Il server utilizza un pool SQLite con un massimo di 50 connessioni e canali con capacità limitata per la comunicazione interna. Anche i dati caricati periodicamente dalla console sono limitati: la dashboard mostra gli ultimi 100 messaggi e il comando `logs` recupera fino a 10 messaggi associati all’utente richiesto.

Non sono stati introdotti benchmark sintetici. La valutazione delle risorse impiegate si basa sul monitoraggio periodico della CPU e sulla dimensione delle build release, mentre la correttezza delle funzionalità viene verificata dalla suite di test.

### 11.3 Dimensione degli eseguibili

Le dimensioni sono state misurate il 12 settembre 2026 dopo l’esecuzione di `cargo build --workspace --release`, su Windows x86-64 con toolchain Rust 1.94.1:

| Eseguibile | Dimensione in byte | Dimensione in MiB |
|---|---:|---:|
| `client.exe` | 4.201.472 | 4,01 |
| `server.exe` | 6.808.064 | 6,49 |

Il crate `common` è una libreria collegata ai due programmi e non produce un eseguibile autonomo. Le dimensioni riportate dipendono dalla piattaforma, dal compilatore e dalle versioni delle dipendenze, quindi possono variare in build eseguite in ambienti differenti.

## 12. Valutazione finale

L’implementazione copre le funzionalità richieste: registrazione e autenticazione, ricezione periodica delle posizioni, gestione degli stati, analisi dei tragitti e comunicazione testuale. La separazione nei tre crate mantiene condiviso il protocollo senza mescolare la logica del client con quella del server. L’uso di Tokio permette inoltre di gestire nello stesso programma connessioni, interfaccia testuale e attività periodiche senza ricorrere a un thread dedicato per ogni operazione.

Le principali scelte progettuali presentano vantaggi e limiti coerenti con le dimensioni del progetto:

| Scelta | Vantaggio | Limite |
|---|---|---|
| Percorsi CSV | Simulazioni ripetibili e facilmente verificabili. | I percorsi devono essere preparati in anticipo e rispettare il formato previsto. |
| SQLite | Persistenza locale semplice e disponibile su entrambe le piattaforme verificate. | Lo schema non dispone di migrazioni versionate e il database rimane legato alla singola istanza del server. |
| Utenti online mantenuti in memoria | La console riflette direttamente le WebSocket attive, senza dipendere da uno stato persistente che potrebbe non essere aggiornato. | L’elenco viene ricostruito a ogni avvio del server. |
| Token mantenuti in memoria | Il client può riutilizzare il token per riconnettersi senza ripetere il login. | I token vengono persi al riavvio e le sessioni devono essere autenticate nuovamente. |
| WebSocket e canali Tokio | Comunicazione bidirezionale senza richieste ripetute e separazione tra messaggi diretti e broadcast. | I broadcast non vengono recuperati dagli utenti che erano offline. |
| Salvataggio al completamento | I dati del tragitto e i relativi punti vengono inseriti in un’unica transazione. | Un tragitto interrotto viene scartato interamente. |
| Interfacce testuali | Uso di librerie multipiattaforma senza dipendenze da un ambiente grafico specifico. | Non è disponibile una rappresentazione grafica dei percorsi. |
| Test unitari e CI multipiattaforma | Le regole principali vengono verificate automaticamente nelle esecuzioni previste dalla pipeline. | Mancano test end-to-end tra processi client e server reali. |

Alcune informazioni sono già predisposte per sviluppi successivi. In particolare, la tabella `trips_points` conserva tutte le coordinate dei tragitti completati, anche se l’interfaccia attuale utilizza soltanto i valori aggregati presenti in `trips`. Questi dati potrebbero essere impiegati per ricostruire e visualizzare graficamente i percorsi.

Ulteriori evoluzioni potrebbero riguardare la persistenza o la scadenza dei token, l’introduzione di migrazioni per lo schema SQLite, la configurazione esterna dei percorsi e test di integrazione completi. La soluzione attuale mantiene però un perimetro adeguato al progetto: le componenti principali sono separate, le regole sul movimento sono verificabili e i dati conclusivi vengono salvati senza lasciare registrazioni parziali.
