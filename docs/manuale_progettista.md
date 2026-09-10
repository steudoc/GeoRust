# GeoRust - Manuale del progettista<!-- omit in toc -->

## Indice<!-- omit in toc -->

- [1. Introduzione](#1-introduzione)
- [2. Analisi dei requisiti](#2-analisi-dei-requisiti)
- [3. Architettura del sistema](#3-architettura-del-sistema)
- [4. Protocollo client-server](#4-protocollo-client-server)
- [5. Simulazione del movimento](#5-simulazione-del-movimento)
- [6. Macchina a stati del tragitto](#6-macchina-a-stati-del-tragitto)
- [7. Persistenza e modello dei dati](#7-persistenza-e-modello-dei-dati)
- [8. Sistema di messaggistica](#8-sistema-di-messaggistica)
- [9. Interfacce testuali](#9-interfacce-testuali)
- [10. Verifica e test](#10-verifica-e-test)
- [11. Prestazioni e dimensione degli eseguibili](#11-prestazioni-e-dimensione-degli-eseguibili)
- [12. Valutazione finale](#12-valutazione-finale)

## 1. Introduzione

GeoRust è un’applicazione client/server sviluppata in Rust per simulare la geolocalizzazione e la comunicazione di una flotta di veicoli. Ogni client rappresenta un utente registrato che, dopo l’autenticazione, può avviare un tragitto, trasmettere periodicamente la propria posizione e inviare messaggi al server.

Il movimento viene simulato leggendo una sequenza di coordinate da un file CSV e le posizioni vengono inviate al server a intervalli di 30 secondi. Durante il tragitto, il server determina lo stato dell’utente, calcola la distanza percorsa e distingue il tempo trascorso in movimento da quello trascorso in pausa.

Il server gestisce inoltre la registrazione e l’autenticazione degli utenti, la persistenza dei dati in un database SQLite, l’elaborazione delle statistiche e lo scambio di messaggi diretti o broadcast. Sia il client sia il server dispongono di un’interfaccia testuale realizzata con Ratatui e Crossterm.

L’applicazione utilizza Tokio per coordinare le attività che devono procedere contemporaneamente. Sul server vengono gestite le richieste HTTP, le connessioni WebSocket, la console amministrativa e il logger della CPU; sul client il runtime coordina l’input dell’utente, la simulazione del percorso, la ricezione dei messaggi e i tentativi di riconnessione, senza bloccare l’interfaccia.

## 2. Analisi dei requisiti

| Requisito | Soluzione adottata |
|---|---|
| Registrazione degli utenti tramite account e password | Il server espone due API REST per la registrazione e il login. Le password vengono salvate sotto forma di hash generato con Argon2. |
| Geolocalizzazione degli utenti | Il client legge le coordinate da percorsi CSV e le invia al server attraverso una connessione WebSocket. |
| Invio della posizione ogni 30 secondi | Ogni punto del percorso contiene un tempo logico. Il client e il server verificano che le posizioni siano distanziate esattamente di 30 secondi. |
| Gestione degli stati dell’utente | Il server utilizza gli stati `Disconnected`, `Moving` e `Still`. Il passaggio a `Moving` avviene al primo cambiamento di coordinate, mentre il passaggio a `Still` avviene dopo tre minuti senza variazioni delle coordinate inviate. |
| Analisi del movimento | Per ogni tragitto vengono calcolati la distanza percorsa, il tempo di movimento, il tempo di pausa e la velocità media. |
| Intervalli temporali programmabili | La console amministrativa del server consente di richiedere le statistiche relative al giorno, alla settimana o al mese corrente di uno specifico utente. |
| Comunicazione con gli utenti | Il server può inviare messaggi diretti oppure broadcast ai client. I client possono inviare messaggi testuali solamente al server. |
| Controllo del consumo di CPU | Il server registra ogni 120 secondi l’utilizzo della CPU, una stima del tempo CPU impiegato nell’ultimo intervallo e la stima cumulativa dall’avvio del processo. |
| Esecuzione su almeno due piattaforme | La compilazione e i controlli automatici vengono eseguiti tramite GitHub Actions su Windows e Ubuntu. |

Per rendere la simulazione ripetibile è stata scelta l’emulazione tramite file CSV. Questa soluzione permette di utilizzare sempre gli stessi percorsi e di verificare con precisione i risultati prodotti dal server. Durante lo sviluppo è stato inoltre introdotto un fattore di velocità, disponibile nelle build di debug, che consente di accelerare la simulazione senza modificare il tempo logico del tragitto.

I dati relativi agli utenti, ai tragitti e ai messaggi vengono salvati in SQLite. Le informazioni che dipendono dalle connessioni attive, come gli utenti online e i token di autenticazione, vengono invece mantenute in memoria dal server.

## 3. Architettura del sistema

GeoRust è organizzato come un workspace Cargo composto da tre crate: `common`, `client` e `server`. Questa suddivisione permette di separare le responsabilità principali e di condividere tra client e server i tipi comuni utilizzati nella comunicazione.

Il crate `common` contiene i tipi utilizzati da entrambe le applicazioni: al suo interno sono definiti i dati scambiati durante la registrazione e il login, i messaggi del protocollo WebSocket, gli stati dell’utente e il tipo che rappresenta una coordinata geografica. La condivisione di questi tipi riduce il rischio che client e server interpretino diversamente lo stesso messaggio.

Il crate `client` gestisce l’interazione con l’utente: si occupa dell’autenticazione tramite richieste HTTP, della selezione e validazione dei percorsi CSV, della simulazione temporale del movimento e della comunicazione con il server attraverso WebSocket.


Il crate `server` espone le API REST e l’endpoint WebSocket, autentica le connessioni dei client, mantiene l’elenco degli utenti collegati, gestisce i tragitti attivi e salva nel database quelli completati. Si occupa inoltre delle statistiche, della messaggistica, della console amministrativa e della registrazione periodica dell’utilizzo della CPU.

La comunicazione utilizza due protocolli differenti:
1.  Le operazioni di registrazione e login vengono eseguite tramite endpoint REST su HTTP, perché sono richieste isolate. 
2.  Dopo l’autenticazione viene aperta una WebSocket, più adatta allo scambio bidirezionale e continuativo di posizioni e messaggi.

## 4. Protocollo client-server

## 5. Simulazione del movimento

## 6. Macchina a stati del tragitto

## 7. Persistenza e modello dei dati

## 8. Sistema di messaggistica

## 9. Interfacce testuali

## 10. Verifica e test

## 11. Prestazioni e dimensione degli eseguibili

## 12. Valutazione finale
