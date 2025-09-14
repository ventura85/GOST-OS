#!/bin/bash
# Wersja 1.2 - Skrypt instalujący pakiety dla pulpitu GOST OS

echo ">>> Instalowanie podstawowych narzędzi..."
sudo apt install -y git sudo

echo ">>> Instalowanie środowiska graficznego XFCE..."
sudo apt install -y xfce4 xfce4-goodies

echo ">>> Instalowanie zależności dla motywów i dodatków..."
sudo apt install -y librsvg2-bin

echo ">>> Instalowanie wyszukiwarki aplikacji..."
sudo apt install -y xfce4-appfinder

echo ">>> Zakończono instalację podstawowych pakietów."