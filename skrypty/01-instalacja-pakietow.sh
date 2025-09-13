#!/bin/bash
# Wersja 1.1 - Skrypt instalujący pakiety dla pulpitu GOST OS

echo ">>> Instalowanie podstawowych narzędzi..."
sudo apt install -y git sudo

echo ">>> Instalowanie środowiska graficznego XFCE..."
sudo apt install -y xfce4 xfce4-goodies

echo ">>> Instalowanie zależności dla motywu Windows 10..."
sudo apt install -y gtk2-engines-murrine gtk2-engines-pixbuf

echo ">>> Instalowanie wyszukiwarki aplikacji..."
sudo apt install -y xfce4-appfinder

echo ">>> Zakończono instalację podstawowych pakietów."