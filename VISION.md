1: **Postilla** è un nome che mi piace molto. È corto, italiano, memorabile e comunica l'idea di annotazioni e note, senza limitare il prodotto alle riunioni.

Il termine "postilla" richiama infatti una nota aggiunta a un testo o a un documento: è coerente con un assistente AI che ascolta, trascrive, riassume e organizza informazioni.

## Vision

> **Postilla** è un assistente AI desktop, local-first e orientato alla privacy, progettato per trasformare conversazioni, riunioni e registrazioni audio in conoscenza organizzata e interrogabile.
>
> L'applicazione funziona interamente sul computer dell'utente, senza backend proprietario. L'utente sceglie liberamente il provider AI (Ollama, ChatGPT, Claude, Gemini, Azure OpenAI, OpenRouter o provider compatibili OpenAI) e mantiene sempre il controllo dei propri dati.

---

# Claim

Alcune idee:

* **Postilla — Your AI Meeting Companion**
* **Postilla — Every conversation becomes knowledge**
* **Postilla — Private AI Notes**
* **Postilla — Capture. Understand. Remember.**
* **Postilla — The Local AI Meeting Assistant**

La mia preferita è:

> **Capture. Understand. Remember.**

È semplice e descrive perfettamente il flusso del prodotto.

---

# Mission

Postilla non è un semplice registratore.

È un assistente desktop che aiuta a:

* registrare riunioni
* trascrivere audio
* generare riassunti
* estrarre attività
* produrre verbali
* organizzare le informazioni
* consultare rapidamente il contenuto delle conversazioni tramite AI

Il tutto senza dipendere da un servizio cloud proprietario.

---

# Principi di progettazione

1. **Local First**
2. **Privacy by Design**
3. **Provider Agnostic**
4. **Plugin First**
5. **Offline Friendly**
6. **Open Architecture**
7. **Cross Platform**
8. **Fast and Lightweight**

---

# Cosa rende Postilla diverso

Non voglio che venga percepito come "l'ennesimo clone di Meetily". Il messaggio dovrebbe essere diverso.

**Meetily** è principalmente un registratore di meeting con AI.

**Postilla** è una piattaforma desktop per la gestione della conoscenza personale.

Le riunioni sono solo uno dei casi d'uso.

Domani potrebbe acquisire informazioni da:

* riunioni
* memo vocali
* lezioni universitarie
* podcast
* video
* interviste
* dettatura
* file audio caricati dall'utente

Tutto finisce nello stesso spazio di lavoro.

---

# Visione futura

Immagino Postilla come qualcosa di simile a:

> **Obsidian + AI + Meeting Assistant**

Un'unica applicazione in cui l'utente può:

* registrare una riunione;
* ottenere automaticamente verbale, decisioni e attività;
* cercare informazioni tra tutte le registrazioni;
* conversare con il proprio archivio di conoscenza;
* esportare facilmente i contenuti;
* scegliere il modello AI preferito senza alcun vincolo.

---

## Una scelta architetturale che suggerisco fin dall'inizio

Anche se il primo MVP sarà focalizzato sulle riunioni, eviterei di modellare il database attorno al concetto di **Meeting**. Userei invece un'entità più generica, ad esempio **Session**.

Una `Session` può rappresentare:

* una riunione;
* una registrazione vocale;
* una lezione;
* un'intervista;
* un file audio importato.

Il tipo (`meeting`, `voice_note`, `lecture`, `import`, ecc.) sarebbe solo un attributo della sessione. In questo modo l'architettura rimane flessibile e non dovrai rifattorizzare il modello dati quando vorrai ampliare le funzionalità del prodotto.

Questa è una di quelle decisioni iniziali che costa poco oggi ma evita molti problemi in futuro.
