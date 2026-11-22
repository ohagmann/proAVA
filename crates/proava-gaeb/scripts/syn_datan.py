#!/usr/bin/env python3
# Aufruf: syn_datan.py crates/proava-gaeb/tests/data
# Erzeugt die synthetischen GAEB-90-Testdateien syn1_33.D83/.D84 (gleiches LV wie syn1_33.X83/.X84,
# ohne Zuschlagsposition 02.01.0030, ohne Tabelle/Bild). Spaltenlagen wie in crates/proava-gaeb/src/datan/tests.rs.
import sys
def r(kind, fields=()):
    line=[' ']*74
    for i,c in enumerate(kind): line[i]=c
    for col,text in fields:
        for i,c in enumerate(text): line[col-1+i]=c
    return ''.join(line)
def out(path, recs):
    data=''.join(f"{rec:<74}{i+1:06}\r\n" for i,rec in enumerate(recs))
    open(path,'wb').write(data.encode('cp850'))
def qty(q, unit): return f"{round(q*1000):011d}{unit}"
head=lambda dp,extra: [r("00",[(11,dp),(14,"V-2026-001"),(63,"1122PPPPI"),(72,"90")]),
    r("01",[(3,"Prüf-LV Rohbau und Ausbau"),(43,"17.11.26"),(51,"15.12.26"),(59,"10:00")]),
    r("02",[(3,"Synthetisches Prüf-LV proAVA")]),
    r("03",[(3,"Stadt Musterstadt")])]+extra+[r("08",[(3,"EUR"),(9,"Euro")])]
items=[ # oz, flags, aln, qty, unit, short, long
 ("01010010","NNN",None,125.5,"m3","Bodenaushub BK 3-5",["Boden lösen, laden und abfahren, Bodenklasse 3-5."]),
 ("01010020","NMN",None,10,"St","Bedarfsposition mit GB",["Bedarfsposition mit Gesamtbetrag."]),
 ("01010030","NEN",None,5,"St","Bedarfsposition ohne GB",["Bedarfsposition ohne Gesamtbetrag."]),
 ("01010040","GNN","001",20,"m","Drainrohr DN 100",["Grundposition der Gruppe 1."]),
 ("01010050","ANN","0011",20,"m","Drainrohr DN 150",["Wahlposition zu 0040."]),
 ("01010060A","NNN",None,1,"psch","Baustelle einrichten",["Baustelleneinrichtung, Index A."]),
 ("01010060B","NNN",None,1,"psch","Baustelle räumen",["Baustelle räumen, Index B."]),
 ("01020010","SNN",None,8,"h","Facharbeiterstunde",["Facharbeiter auf Nachweis."]),
 ("02010010","NNN",None,40,"m2","Bodenbelag",["Bodenbelag liefern und verlegen,"],),
 ("02010020","NNN",None,3,"St","Bodentür mit Skizze",["Bodentür nach Skizze:"]),
]
groups={"01":("Titel","Rohbau"),"0101":("Untertitel","Erdarbeiten"),"0102":("Untertitel","Stundenlohnarbeiten"),"02":("Titel","Ausbau"),"0201":("Untertitel","Bodenbeläge")}
order=[("open","01"),("open","0101")]+[("item",i) for i in range(7)]+[("close","0101"),("open","0102"),("item",7),("close","0102"),("close","01"),("open","02"),("open","0201"),("item",8),("item",9),("close","0201"),("close","02")]
d83=head("83",[])+[r("20"),r("26",[(6,"Vorbemerkung: Alle Mengen sind Vordersätze.")])]
for what,x in order:
    if what=="open":
        lbl,txt=groups[x]; d83+= [r("11",[(3,x),(12,"N"),(17,lbl)]), r("12",[(3,txt)])]
    elif what=="close": d83.append(r("31",[(3,x)]))
    else:
        oz,fl,aln,q,u,st,lt=items[x]
        f=[(3,oz),(12,fl),(24,qty(q,u))]
        if aln: f.append((15,aln))
        d83+= [r("21",f), r("25",[(3,st)])]+[r("26",[(6,l)]) for l in lt]
        if oz=="02010010":
            d83+= [r("26",[(3,"31AFarbe 'grau'")]), r("26",[(3,"61BFabrikat '....................'")])]
d83.append(r("99",[(70,f"{len(items):05d}")]))
out(sys.argv[1]+"/syn1_33.D83", d83)
prices={"01010010":(18.4,2309.20),"01010020":(55,550.00),"01010030":(80,None),"01010040":(12.5,250.00),"01010050":(14,None),
 "01010060A":(1500,1500.00),"01010060B":(750,750.00),"01020010":(52.5,420.00),"02010010":(21.75,870.00),"02010020":(199.99,599.97)}
tot={"0101":5359.20,"0102":420.00,"01":5779.20,"0201":1469.97,"02":1469.97}
def up(v): return f"{round(v*1000):011d}"
def money(v): return f"{round(v*100):012d}"
d84=head("84",[r("04",[(3,"Muster Bau GmbH")])])
for what,x in order:
    if what=="close": d84.append(r("32",[(3,x),(13,money(tot[x])),(25,"1000000")]))
    elif what=="item":
        oz=items[x][0]; p,it=prices[oz]
        f=[(3,oz),(13,up(p))]
        if it is not None: f.append((25,money(it)))
        d84.append(r("23",f))
        if oz=="02010010": d84.append(r("26",[(3,"61BFabrikat 'Musterbelag 2000'")]))
total=7249.17
d84.append(r("99",[(3,money(total)),(15,"1000000"),(22,"1900"),(26,money(round(total*1.19,2))),(70,f"{len(items):05d}")]))
out(sys.argv[1]+"/syn1_33.D84", d84)
